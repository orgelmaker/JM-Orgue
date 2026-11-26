"""Read-only installed-source probe; project/profile output stays in --work-dir.

No notes are played and no source samples or source-organ settings are written.
Exit 2 reports an unsupported/missing source without trying another source.
"""
import argparse
import json
import os
from pathlib import Path
import sys
import uuid

from console_texture_background_test import Child, digest, norm, read_json

PURPOSE = 'jm-orgue-installed-organ-project-test'


def settings_manifest(sources):
    return {str(file): digest(file) for source in sources
            for file in sorted(source.parent.glob('.jm-settings*')) if file.is_file()}


def main():
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('exe', 'work-dir', 'source-a', 'source-b'):
        parser.add_argument('--' + name, required=True, type=Path)
    args = parser.parse_args()
    assert all(path.is_absolute() for path in (args.exe, args.work_dir, args.source_a, args.source_b))
    exe, work = args.exe.resolve(strict=True), args.work_dir.resolve()
    assert exe.is_file() and exe.name.lower() in ('vpo-app.exe', 'vpo-app')
    allowed = (Path(__file__).resolve().parents[2] / 'testen' / 'orgelbouwer-2026-11-26').resolve()
    work.relative_to(allowed)
    marker = work / '.jm-installed-organ-project-test'
    if work.exists():
        assert marker.is_file() and read_json(marker) == {'purpose': PURPOSE}, 'Unmarked work directory refused'
    else:
        work.mkdir(parents=True)
        marker.write_text(json.dumps({'purpose': PURPOSE}), encoding='utf-8')
    with (work / '.running').open('x', encoding='utf-8') as file:
        file.write(str(os.getpid()))
    result, child, exit_code = {'status': 'running', 'checks': []}, None, 1
    sources = [args.source_a.resolve(), args.source_b.resolve()]
    before = {str(source): digest(source) for source in sources if source.is_file()}
    settings_before = settings_manifest(sources)
    stage = 'source files'
    try:
        assert norm(sources[0]) != norm(sources[1]), 'Two different definitions required'
        for source in sources:
            assert source.is_file(), f'Source does not exist: {source}'
        result['definitions_before'], result['settings_before'] = before, settings_before
        profile = work / 'profile'
        profile.mkdir(exist_ok=True)
        assert profile.resolve().parent == work and not profile.is_symlink()
        stage = 'headless child startup'
        child = Child(exe, profile, work / 'headless-child.log')
        result['process'] = {'pid': child.process.pid, 'port': child.port}
        child.api('/settings/master?db=-40', {})
        stage = 'source analysis'
        catalogs = [child.api('/project/source', {'path': str(source), 'package_root': None}) for source in sources]
        result['catalogs'] = catalogs
        selected = [next((stop for stop in catalog['stops'] if stop['key_count'] > 0), None) for catalog in catalogs]
        assert all(selected), 'A source has no complete playable register in its catalog'
        result['selected_registers'] = selected
        project = {'schema_version': 1, 'id': str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE)),
                   'name': 'Geinstalleerde bronnen — leesproef', 'target_a4_hz': 440,
                   'next_stop_id': 3, 'next_division_id': 3, 'sources': [],
                   'divisions': [], 'stops': [], 'couplers': []}
        for index, (source, catalog, stop) in enumerate(zip(sources, catalogs, selected), 1):
            source_id = str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE + '/' + str(source)))
            project['sources'].append({'id': source_id, 'path': str(source), 'package_root': None,
                'fingerprint': catalog['fingerprint'], 'resources': [], 'perspective': None,
                'reference_a4_hz': 440, 'tuning_policy': 'original', 'tuning_cents': 0})
            project['divisions'].append({'id': index, 'name': f'Bron {index}', 'midi_channel': index + 1,
                'first_midi_note': stop['first_midi_note'], 'key_count': stop['key_count'], 'pedal': False})
            project['stops'].append({'id': index, 'source_id': source_id, 'source_stop_id': stop['id'],
                                    'division_id': index, 'name': None})
        result['tuning_note'] = 'Original for both sources; A references are placeholders, not measurements or normalization.'
        project_path = work / 'Geinstalleerde bronnen.jmorgan'
        stage = 'project validation and save'
        saved = child.api('/project/save', {'path': str(project_path), 'project': project}, timeout=120)
        assert all(source['resources'] for source in saved['sources'])
        result['project'], result['project_sha256'] = str(project_path), digest(project_path)
        stage = 'project load'
        child.api('/load_organ', {'path': str(project_path)}, timeout=180)
        organ = child.api('/organ')
        assert norm(organ['id']) == norm(project_path) and organ['stop_count'] == 2
        loaded = [stop for division in organ['divisions'] for stop in division['stops']]
        assert {stop['internal_stop_id'] for stop in loaded} == {1, 2}
        assert len(organ['divisions']) == 2
        mappings = {mapping['division']: mapping for mapping in child.api('/midi/mapping')['mappings']}
        for index, stop in enumerate(selected, 1):
            mapping = mappings[f'Bron {index}']
            assert mapping['channel'] == index, mappings
            assert mapping['first_midi_note'] == stop['first_midi_note'], mappings
            assert mapping['last_midi_note'] == stop['first_midi_note'] + stop['key_count'] - 1, mappings
        result['midi_mappings'] = mappings
        result['organ'] = organ
        result['checks'] += ['two explicit installed sources inspected', 'one complete register per source saved in an independent project',
                             'two registers loaded together in separate project manuals; no notes played',
                             'project model MIDI channels and full selected-register key ranges applied']
        exit_code, result['status'] = 0, 'passed'
    except BaseException as error:
        unsupported = stage in ('source files', 'source analysis', 'project validation and save')
        result.update(status='unsupported' if unsupported else 'failed', stage=stage, error=str(error))
        exit_code = 2 if result['status'] == 'unsupported' else 1
    finally:
        if child is not None:
            child.stop()
        result['definitions_after'] = {str(source): digest(source) for source in sources if source.is_file()}
        result['settings_after'] = settings_manifest(sources)
        if result['definitions_after'] != before or result['settings_after'] != settings_before:
            result.update(status='failed', error='Source definition or source-settings manifest changed')
            exit_code = 1
        else:
            result['checks'].append('source definition and .jm-settings* hashes unchanged; owned child stopped')
        (work / 'installed-result.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
        (work / '.running').unlink()
        print(json.dumps({'status': result['status'], 'stage': result.get('stage'),
                          'checks': result['checks'], 'error': result.get('error'), 'result': str(work / 'installed-result.json')}, indent=2))
    return exit_code


if __name__ == '__main__':
    raise SystemExit(main())
