"""Exercise imported dynamic wind valves in owned, windowless app processes.

The fixture has two ODFs in one directory, separate package roots and deliberately
overlapping compartment, switch and continuous-control IDs. A sustained sounding
stop is separate from the selected stop that controls the binary valve, so a
pressure drop cannot be explained by releasing the sounding stop.
"""
import argparse
import copy
import json
import os
from pathlib import Path
import sys
import time
import uuid
from xml.etree.ElementTree import Element, SubElement, tostring

from console_texture_background_test import Child, digest
from organ_project_background_test import make_wave, organ_text, write_fixture


PURPOSE = 'jm-orgue-project-valve-background-test'
ALLOWED_DIR = 'orgelbouwer-kleppen-2026-11-26'
SOURCE_IDS = [str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE + '/' + letter)) for letter in ('A', 'B')]
PROJECT_ID = str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE))


def is_unknown_linkage_error(value):
    message = str(value).lower()
    return 'linktypecode' in message and 'niet ondersteund' in message


def xml(letter, automated=False):
    """Use the actual HW ODF field names, without proprietary sample material."""
    document = Element('Hauptwerk', FileFormat='Organ', FileFormatVersion='4.0')

    def table(kind, rows):
        objects = SubElement(document, 'ObjectList', ObjectType=kind)
        for values in rows:
            row = SubElement(objects, kind)
            for field, value in values.items():
                SubElement(row, field).text = str(value)

    table('_General', [{'Identification_Name': 'Kleppenbron ' + letter,
                         'EnablePlayingAtOriginalOrganPitch': 'Y'}])
    table('Division', [{'DivisionID': 1, 'Name': 'Bronmanual'}])
    table('Switch', [{'SwitchID': 7, 'Name': 'Windschakelaar ' + letter,
                      'DefaultToEngaged': 'N' if letter == 'A' else 'Y'}])
    continuous_controls = [{'ControlID': 9, 'Name': 'Windregelaar ' + letter,
                            'DefaultValue': 96 if letter == 'A' else 127}]
    if automated:
        continuous_controls.append({'ControlID': 8, 'Name': 'Bronautomatisering', 'DefaultValue': 0})
    table('ContinuousControl', continuous_controls)
    if automated:
        # An explicitly unknown linkage code must remain rejected even when
        # the supported direct, Willis and inertia models are available.
        table('ContinuousControlLinkage', [{
            'SourceControlID': 8, 'DestControlID': 9, 'Name': 'Unknown automation', 'LinkTypeCode': 99,
            'InertiaModelTypePositiveAcceleratingCoeff': 1000,
            'InertiaModelTypePositiveDampingCoeff': 1,
        }])
    table('Stop', [
        {'StopID': 1, 'Name': 'Klinkende grondstem 8', 'DivisionID': 1},
        {'StopID': 2, 'Name': 'Windklepbediening', 'DivisionID': 1, 'ControllingSwitchID': 7},
    ])
    # The valve stop has a real mapped rank at note 70. It remains silent during
    # the note-69 pressure test; the sound and its consumption continue unchanged.
    table('StopRank', [
        {'StopID': stop, 'RankID': stop, 'MIDINoteNumOfFirstMappedDivisionInputNode': note,
         'NumberOfMappedDivisionInputNodes': 1}
        for stop, note in ((1, 69), (2, 70))
    ])
    table('Rank', [{'RankID': rank, 'Name': 'Grondstem ' + str(rank)} for rank in (1, 2)])
    table('Pipe_SoundEngine01', [
        {'PipeID': rank, 'RankID': rank, 'NormalMIDINoteNumber': note,
         'Pitch_Tempered_RankBasePitch64ftHarmonicNum': 8,
         'Pitch_OriginalOrgan_SpecificationMethodCode': 2,
         'Pitch_OriginalOrgan_PitchHz': 440,
         'WindSupply_SourceWindCompartmentID': 3,
         'WindSupply_MassFlowRateKilogramsPerSecAtReferencePressureDiff': 0.008,
         'WindSupply_ReferencePressureDifferenceInches': 4}
        for rank, note in ((1, 69), (2, 70))
    ])
    table('Pipe_SoundEngine01_Layer', [
        {'LayerID': rank, 'PipeID': rank, 'PipeLayerNumber': 1} for rank in (1, 2)
    ])
    table('Pipe_SoundEngine01_AttackSample', [
        {'UniqueID': rank, 'LayerID': rank, 'SampleID': 1} for rank in (1, 2)
    ])
    table('Sample', [{'SampleID': 1, 'InstallationPackageID': 1,
                      'SampleFilename': 'tone.wav', 'Pitch_SpecificationMethodCode': 4,
                      'Pitch_ExactSamplePitch': 440}])
    table('WindCompartment', [
        {'WindCompartmentID': 1, 'Name': 'Ventilator', 'InfiniteVolume': 'Y',
         'DefaultAirPressureInches': 4},
        {'WindCompartmentID': 2, 'Name': 'Tussenkamer', 'InfiniteVolume': 'N',
         'StandardVolumeMetresCubed': 0.08, 'DefaultAirPressureInches': 4},
        {'WindCompartmentID': 3, 'Name': 'Windlade', 'InfiniteVolume': 'N',
         'StandardVolumeMetresCubed': 0.08, 'DefaultAirPressureInches': 4},
    ])
    table('WindCompartmentLinkage', [
        {'FirstWindCompartmentID': 1, 'SecondWindCompartmentID': 2,
         'MassFlowRateKilogramsPerSecAtReferencePressureDiff': 0.02,
         'ReferencePressureDifferenceInches': 1, 'ValveControlTypeCode': 2,
         'ValveControllingSwitchID': 7,
         'ValveOpenIfControllingSwitchEngaged': 'N' if letter == 'A' else 'Y'},
        {'FirstWindCompartmentID': 2, 'SecondWindCompartmentID': 3,
         'MassFlowRateKilogramsPerSecAtReferencePressureDiff': 0.02,
         'ReferencePressureDifferenceInches': 1, 'ValveControlTypeCode': 3,
         'ValveControllingContinuousControlID': 9},
    ])
    return tostring(document, encoding='utf-8', xml_declaration=True)


def prepare(work):
    sources = work / 'sources'
    sources.mkdir(exist_ok=True)
    paths, roots = [], []
    for letter in ('A', 'B'):
        root = sources / ('packages-' + letter)
        package = root / 'OrganInstallationPackages' / '000001'
        package.mkdir(parents=True, exist_ok=True)
        make_wave(package / 'tone.wav', 48000, 440.0, False)
        write_fixture(package / 'PackageID000001.InstallationPackageDefinition_Hauptwerk_xml', b'<Hauptwerk/>')
        path = sources / ('Kleppen ' + letter + '.Organ_Hauptwerk_xml')
        write_fixture(path, xml(letter))
        paths.append(path)
        roots.append(root)
    ordinary = sources / 'Gewone controle.organ'
    write_fixture(ordinary, organ_text('gewoon', ['Grondstem 8'],
                                     ['packages-A/OrganInstallationPackages/000001/tone.wav']))
    unsupported = sources / 'Niet ondersteunde klepautomatisering.Organ_Hauptwerk_xml'
    write_fixture(unsupported, xml('A', automated=True))
    originals = {str(file): digest(file) for file in sources.rglob('*') if file.is_file()}
    return paths, roots, ordinary, unsupported, originals


def main():
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', required=True, type=Path)
    parser.add_argument('--work-dir', required=True, type=Path)
    parser.add_argument('--prepare-only', action='store_true', help='Write the fixture without starting any process')
    args = parser.parse_args()
    assert args.exe.is_absolute() and args.work_dir.is_absolute()
    exe, work = args.exe.resolve(strict=True), args.work_dir.resolve()
    allowed = Path(__file__).resolve().parents[2] / 'testen' / ALLOWED_DIR
    work.relative_to(allowed.resolve())
    assert exe.name.lower() in ('vpo-app.exe', 'vpo-app')
    work.mkdir(parents=True, exist_ok=True)
    assert not work.is_symlink()
    write_fixture(work / '.jm-project-valve-fixture', (PURPOSE + '\n').encode())
    lock = work / '.running'
    with lock.open('x', encoding='utf-8') as file:
        file.write(str(os.getpid()))
    result = {
        'status': 'running', 'checks': [], 'processes': [], 'snapshots': {}, 'audio': [],
        'model_notes': [{
            'limitation': 'The source-wind graph is a bounded pneumatic approximation, including a conservative equalising-flow cap.',
            'exploratory_run': str(allowed / 'valves-1' / 'valve-result.json'),
            'exploratory_chamber_volume_m3': .04,
            'exploratory_full_open_pressure_ratio': .917962908744812,
            'uncapped_analytical_full_open_pressure_ratio': 1 / 1.08,
            'final_fixture_chamber_volume_m3': .08,
            'explanation': 'The smaller exploratory chambers reached the conservative flow cap. The final fixture uses larger chambers so the uncapped analytical steady-state comparison is applicable. The solver was not changed for this test.',
        }],
    }
    child = None

    def check(name):
        result['checks'].append(name)
        print('OK ' + name, flush=True)

    def snapshot(label):
        value = child.api('/project/wind')
        result['snapshots'][label] = value
        return value

    def wait_for(predicate, label, seconds=5):
        deadline = time.monotonic() + seconds
        value = None
        while time.monotonic() < deadline:
            value = child.api('/project/wind')
            if predicate(value):
                result['snapshots'][label] = value
                return value
            time.sleep(.04)
        raise AssertionError(f'{label}: {value}')

    def quiet():
        child.api('/settings/master?db=-40', {})
        child.api('/settings/wind?enabled=0', {})

    def stop_notes():
        child.api('/notes/69/off', {})
        child.api('/panic', {})
        # Panic purges queued notes in the following audio callback.
        time.sleep(.05)

    def record_held(label):
        recordings = work / 'recordings'
        recordings.mkdir(exist_ok=True)
        path = recordings / (label + '.mp3')
        child.api('/record/start', {'path': str(path)})
        try:
            time.sleep(.65)
        finally:
            value = child.api('/record/stop', {})
        assert value['decoded_samples'] > 0 and value['frames_dropped'] == 0, value
        result['audio'].append({'label': label, 'recording': str(path), **value})
        return value['rms']

    try:
        paths, roots, ordinary, unsupported, originals = prepare(work)
        assert paths[0].parent == paths[1].parent and roots[0] != roots[1]
        if args.prepare_only:
            result['status'] = 'prepared'
            check('two source definitions and isolated sample packages prepared without starting the app')
            return
        profile = work / 'profile'
        profile.mkdir(exist_ok=True)
        child = Child(exe, profile, work / 'child-1.log')
        result['processes'].append(child.process.pid)
        catalogs = [child.api('/project/source', {'path': str(path), 'package_root': str(root)})
                    for path, root in zip(paths, roots)]
        assert all(len(catalog['stops']) == 2 for catalog in catalogs), catalogs
        assert not any('statisch open' in warning for catalog in catalogs for warning in catalog['warnings'])
        check('source inspection recognizes both supported dynamic valve types')
        project = {
            'schema_version': 1, 'id': PROJECT_ID, 'name': 'Twee onafhankelijke kleppenbronnen',
            'target_a4_hz': 440, 'next_stop_id': 203, 'next_division_id': 2, 'sources': [],
            'divisions': [{'id': 1, 'name': 'Gedeeld manual', 'midi_channel': 2,
                           'first_midi_note': 69, 'key_count': 2, 'pedal': False}],
            'stops': [], 'couplers': [],
        }
        for index, (path, root, catalog, source_id) in enumerate(zip(paths, roots, catalogs, SOURCE_IDS), 1):
            project['sources'].append({
                'id': source_id, 'path': str(path), 'package_root': str(root),
                'fingerprint': catalog['fingerprint'], 'resources': [], 'reference_a4_hz': 440,
                'tuning_policy': 'original', 'tuning_cents': 0, 'perspective': None,
            })
            for source_stop in (1, 2):
                project['stops'].append({'id': index * 100 + source_stop, 'source_id': source_id,
                                         'source_stop_id': source_stop, 'division_id': 1,
                                         'name': 'Bron ' + str(index) + ' register ' + str(source_stop)})
        project_path = work / 'Twee dynamische windnetwerken.jmorgan'
        child.api('/project/save', {'path': str(project_path), 'project': project})
        child.api('/load_organ', {'path': str(project_path)})
        quiet()
        initial = wait_for(lambda value: len(value['pressure_ratios']) == 6 and len(value['controls']) == 4, 'loaded')
        assert len(initial['nodes']) == 6 and len(initial['links']) == 4 and initial['pipe_count'] == 4
        assert not any('statisch open' in warning for warning in initial['warnings'])
        controls = {control['id']: index for index, control in enumerate(initial['controls'])}
        switch_ids = [source_id + ':switch:7' for source_id in SOURCE_IDS]
        continuous_ids = [source_id + ':continuous:9' for source_id in SOURCE_IDS]
        assert len(controls) == 4 and set(controls) == set(switch_ids + continuous_ids), controls
        for source, stop in zip(SOURCE_IDS, (102, 202)):
            control = initial['controls'][controls[source + ':switch:7']]
            assert control['source_id'] == source and control['project_stop_ids'] == [stop], control
        nodes = [next(index for index, node in enumerate(initial['nodes'])
                      if node['source_id'] == source_id and node['compartment_id'] == 3)
                 for source_id in SOURCE_IDS]

        def ratios(value):
            return [value['pressure_ratios'][index] for index in nodes]

        def raw(value, control_id):
            return value['control_values'][controls[control_id]]

        def openings(value, control_id):
            return [value['valve_openings'][index] for index, link in enumerate(value['links'])
                    if link['control'] == controls[control_id]]

        def flows(value, control_id):
            return [value['link_flows'][index] for index, link in enumerate(value['links'])
                    if link['control'] == controls[control_id]]

        def loaded_ratio(continuous_opening):
            # A .008 kg/s pipe at 4 inches, supplied through two .02 kg/s
            # links at 1 inch: two square-root pressure drops in series.
            return 1 / (1.04 + .04 / continuous_opening ** 2)

        def set_control(control_id, value):
            response = child.api('/project/wind/control', {'control_id': control_id, 'value': value})
            assert response.get('warning') is None, response
            assert abs(child.api('/project/wind/controls')['values'][control_id] - value) < .00001
            return response

        defaults = [initial['controls'][controls[control_id]]['default_value']
                    for control_id in switch_ids + continuous_ids]
        assert all(abs(a - b) < .00001 for a, b in zip(defaults, (0, 1, 96 / 127, 1))), defaults
        default_state = wait_for(lambda value: abs(raw(value, switch_ids[0])) < .001
                                 and abs(raw(value, switch_ids[1]) - 1) < .001
                                 and abs(raw(value, continuous_ids[0]) - 96 / 127) < .001, 'source-defaults')
        assert openings(default_state, switch_ids[0]) == [1]
        assert openings(default_state, switch_ids[1]) == [1]
        assert abs(openings(default_state, continuous_ids[0])[0] - 96 / 127) < .001
        check('overlapping raw controller IDs remain source-local and source defaults are restored')

        default_targets = child.api('/project/wind/controls')
        rejected = [
            ('linked stop controller', {'control_id': switch_ids[0], 'value': 1}),
            ('unknown controller', {'control_id': SOURCE_IDS[0] + ':continuous:999', 'value': .5}),
            ('negative opening', {'control_id': continuous_ids[0], 'value': -.01}),
            ('opening above one', {'control_id': continuous_ids[0], 'value': 1.01}),
            ('NaN JSON opening', {'control_id': continuous_ids[0], 'value': float('nan')}),
            ('infinite JSON opening', {'control_id': continuous_ids[0], 'value': float('inf')}),
            ('stale organ identity', {'control_id': continuous_ids[0], 'value': .5, 'organ_id': str(ordinary)}),
        ]
        for label, body in rejected:
            child.api('/project/wind/control', body, expected=400)
            assert child.api('/project/wind/controls') == default_targets, label
            unchanged = snapshot('rejected-' + label.replace(' ', '-'))
            assert unchanged['controls'] == default_state['controls'], label
            assert len(unchanged['control_values']) == len(default_state['control_values']), label
            assert len(unchanged['valve_openings']) == len(default_state['valve_openings']), label
            assert all(abs(a - b) < .00001 for a, b in
                       zip(unchanged['control_values'], default_state['control_values'])), label
            assert all(abs(a - b) < .00001 for a, b in
                       zip(unchanged['valve_openings'], default_state['valve_openings'])), label
        check('linked, unknown, out-of-range, non-finite and stale-organ writes are rejected without changing valve state')

        unsupported_catalog = child.api('/project/source', {'path': str(unsupported), 'package_root': str(roots[0])})
        assert any(is_unknown_linkage_error(warning) for warning in unsupported_catalog['warnings']), unsupported_catalog
        unsupported_project = copy.deepcopy(project)
        unsupported_project['id'] = str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE + '/unsupported'))
        unsupported_project['sources'][0].update(path=str(unsupported), fingerprint=unsupported_catalog['fingerprint'])
        unsupported_path = work / 'Niet ondersteunde klepautomatisering.jmorgan'
        before_unsupported = child.api('/organ')
        rejected_save = child.api('/project/save', {'path': str(unsupported_path), 'project': unsupported_project}, expected=400)
        assert is_unknown_linkage_error(rejected_save), rejected_save
        assert not unsupported_path.exists()
        write_fixture(unsupported_path, json.dumps(unsupported_project, indent=2).encode())
        rejected_load = child.api('/load_organ', {'path': str(unsupported_path)}, expected=500)
        assert is_unknown_linkage_error(rejected_load), rejected_load
        assert child.api('/organ') == before_unsupported
        assert child.api('/project/wind/controls') == default_targets
        check('unknown control automation is rejected before saving or loading, with the previous organ and controls retained')

        child.api('/stops/set', {'ids': ['project_101', 'project_202']})
        child.api('/notes/69/on?velocity=100', {})
        baseline = wait_for(lambda value: abs(ratios(value)[0] - loaded_ratio(96 / 127)) < .0001
                            and abs(ratios(value)[1] - 1) < .0001,
                            'a-default-loaded')
        check('source A consumes wind while the second source remains at nominal pressure')
        child.api('/stops/set', {'ids': ['project_101', 'project_102', 'project_202']})
        closed = wait_for(lambda value: ratios(value)[0] < .25 and abs(ratios(value)[1] - 1) < .0001,
                          'a-inverted-switch-closed')
        assert raw(closed, switch_ids[0]) > .999 and raw(closed, switch_ids[1]) > .999
        assert openings(closed, switch_ids[0]) == [0] and openings(closed, switch_ids[1]) == [1]
        assert flows(closed, switch_ids[0]) == [0]
        check('engaging the actual mapped project stop closes the inverted binary valve under a sustained note')
        child.api('/stops/set', {'ids': ['project_101', 'project_202']})
        wait_for(lambda value: abs(ratios(value)[0] - loaded_ratio(96 / 127)) < .0001
                 and abs(raw(value, switch_ids[0])) < .001,
                 'a-inverted-switch-reopened')
        check('disengaging the mapped stop reopens the valve and restores pressure without retriggering the note')

        set_control(continuous_ids[0], 0)
        continuous_closed = wait_for(lambda value: ratios(value)[0] < .25
                                     and abs(raw(value, continuous_ids[0])) < .001
                                     and abs(ratios(value)[1] - 1) < .0001, 'a-continuous-closed')
        assert flows(continuous_closed, continuous_ids[0]) == [0]
        set_control(continuous_ids[0], .35)
        partial = wait_for(lambda value: abs(ratios(value)[0] - loaded_ratio(.35)) < .0001
                          and abs(raw(value, continuous_ids[0]) - .35) < .001, 'a-continuous-partial')
        set_control(continuous_ids[0], 1)
        full = wait_for(lambda value: abs(ratios(value)[0] - loaded_ratio(1)) < .0001
                        and abs(raw(value, continuous_ids[0]) - 1) < .001,
                       'a-continuous-open')
        assert ratios(full)[0] > ratios(partial)[0] + .10
        assert abs(raw(full, continuous_ids[1]) - 1) < .001 and abs(ratios(full)[1] - 1) < .0001
        check('continuous closure, partial opening and full opening regulate pressure independently of the second source')

        # Reverse the roles and exercise the non-inverted switch while the first
        # source is silent. This also catches accidental sharing of raw ID 7.
        stop_notes()
        child.api('/stops/set', {'ids': ['project_201', 'project_202']})
        child.api('/notes/69/on?velocity=100', {})
        wait_for(lambda value: abs(ratios(value)[1] - loaded_ratio(1)) < .0001
                 and abs(ratios(value)[0] - 1) < .0001, 'b-open')
        child.api('/stops/set', {'ids': ['project_201']})
        wait_for(lambda value: ratios(value)[1] < .25 and abs(ratios(value)[0] - 1) < .0001
                 and abs(raw(value, switch_ids[1])) < .001, 'b-normal-switch-closed')
        child.api('/stops/set', {'ids': ['project_201', 'project_202']})
        wait_for(lambda value: abs(ratios(value)[1] - loaded_ratio(1)) < .0001
                 and raw(value, switch_ids[1]) > .999, 'b-normal-switch-reopened')
        stop_notes()
        check('a non-inverted switch in the second source responds only to its own mapped project stop')

        # Traverse the real MIDI CC loop and crescendo registration path. This
        # bypasses /stops/set for the valve changes and verifies that all ways
        # of drawing project stops synchronise the source switch in audio.
        set_control(continuous_ids[0], 1)
        child.api('/stops/set', {'ids': ['project_101', 'project_202']})
        child.api('/crescendo/config', {'stages': [['project_102']], 'enabled': True, 'num_stages': 1})
        child.api('/crescendo/binding', {'channel': 5, 'cc': 11, 'min': 0, 'max': 127, 'invert': False})
        child.api('/notes/69/on?velocity=100', {})
        wait_for(lambda value: abs(ratios(value)[0] - loaded_ratio(1)) < .0001, 'midi-crescendo-before')
        child.api('/midi/inject?cc=11&value=127&channel=5', {})
        wait_for(lambda value: ratios(value)[0] < .25 and raw(value, switch_ids[0]) > .999
                 and abs(ratios(value)[1] - 1) < .0001, 'midi-crescendo-closed')
        assert child.api('/crescendo')['stage'] == 1
        child.api('/midi/inject?cc=11&value=0&channel=5', {})
        wait_for(lambda value: abs(ratios(value)[0] - loaded_ratio(1)) < .0001
                 and raw(value, switch_ids[0]) < .001, 'midi-crescendo-reopened')
        assert child.api('/crescendo')['stage'] == 0
        stop_notes()
        child.api('/crescendo/config', {'stages': [['project_102']], 'enabled': False, 'num_stages': 1})
        child.api('/crescendo/binding', {'clear': True})
        check('real MIDI crescendo CCs draw and release the linked project stop, closing and reopening its source valve')

        # Record the same held voice through open -> drained -> open. No new
        # note, voice or registration can account for the disappearance and
        # recovery of the actual decoded audio output.
        set_control(continuous_ids[0], 1)
        child.api('/reverb', {'mix': 0})
        child.api('/stops/set', {'ids': ['project_101', 'project_202']})
        started = child.api('/notes/69/on?velocity=100', {})
        assert started['voices_triggered'] == 1, started
        wait_for(lambda value: abs(ratios(value)[0] - loaded_ratio(1)) < .0001, 'audio-open-pressure')
        audio_open = record_held('A-open')
        child.api('/stops/set', {'ids': ['project_101', 'project_102', 'project_202']})
        wait_for(lambda value: ratios(value)[0] < .0001, 'audio-drained-pressure')
        audio_closed = record_held('A-dicht')
        child.api('/stops/set', {'ids': ['project_101', 'project_202']})
        wait_for(lambda value: abs(ratios(value)[0] - loaded_ratio(1)) < .0001, 'audio-reopened-pressure')
        audio_reopened = record_held('A-heropend')
        assert audio_open > .00001, result['audio']
        assert audio_closed < audio_open * .01, result['audio']
        assert .9 < audio_reopened / audio_open < 1.1, result['audio']
        stop_notes()
        check('decoded recordings of the same sustained voice become silent at drained pressure and recover when the binary valve reopens')

        # Force only this owned profile's atomic library commit to fail. Keep
        # its exact bytes in a sibling backup and restore them unconditionally;
        # no recursive deletion, process-name lookup or user profile is used.
        library = profile / 'organ_library.json'
        backup = profile / '.organ_library.valve-write-test-backup.json'
        assert library.resolve().parent == profile.resolve() and library.is_file() and not library.is_symlink()
        assert not backup.exists()
        before_library_hash = digest(library)
        before_targets = child.api('/project/wind/controls')
        before_audio_controls = snapshot('before-library-write-failure')
        before_sidecars = {str(file): digest(file) for file in work.glob('.jm-settings-project-*.json')}
        assert before_sidecars, 'The successful opening changes must have created a project sidecar'
        library.rename(backup)
        try:
            library.mkdir()
            failed_write = child.api('/project/wind/control', {'control_id': continuous_ids[0], 'value': .23}, expected=400)
            assert 'vorige stand' in str(failed_write).lower(), failed_write
            assert child.api('/project/wind/controls') == before_targets
            unchanged = snapshot('rejected-library-write-failure')
            assert unchanged['control_values'] == before_audio_controls['control_values']
            assert unchanged['valve_openings'] == before_audio_controls['valve_openings']
            assert {str(file): digest(file) for file in work.glob('.jm-settings-project-*.json')} == before_sidecars
        finally:
            if library.is_dir():
                assert not library.is_symlink()
                library.rmdir()
            assert not library.exists()
            backup.rename(library)
        assert digest(library) == before_library_hash
        check('a failed durable library write preserves target values, audio valve openings and project sidecar bytes')

        # Use the real settings-import command, so an imported valve map must
        # update both the durable settings and the already-running audio graph.
        exported = child.api('/settings/organ')
        assert 'project_wind_controls' in exported, exported
        write_fixture(work / 'settings-export.json', json.dumps(exported, indent=2).encode())
        before_import_targets = child.api('/project/wind/controls')
        before_import_audio = snapshot('before-settings-import')
        before_import_library_hash = digest(library)
        before_import_sidecars = {str(file): digest(file) for file in work.glob('.jm-settings-project-*.json')}
        for label, bad_value in (('out-of-range', 2), ('non-finite', float('nan'))):
            invalid = copy.deepcopy(exported)
            invalid['project_wind_controls'][continuous_ids[0]] = bad_value
            invalid_path = work / ('settings-invalid-' + label + '.json')
            write_fixture(invalid_path, json.dumps(invalid, indent=2).encode())
            child.api('/settings/import', {'path': str(invalid_path), 'organ_id': str(project_path)}, expected=400)
            assert child.api('/project/wind/controls') == before_import_targets, label
            unchanged = snapshot('rejected-settings-import-' + label)
            assert unchanged['control_values'] == before_import_audio['control_values'], label
            assert unchanged['valve_openings'] == before_import_audio['valve_openings'], label
            assert digest(library) == before_import_library_hash, label
            assert {str(file): digest(file) for file in work.glob('.jm-settings-project-*.json')} == before_import_sidecars, label
        check('invalid imported valve maps are rejected before changing saved settings, target values or audio')
        imported = copy.deepcopy(exported)
        imported['project_wind_controls'] = {
            continuous_ids[0]: .7, continuous_ids[1]: .9,
            'removed-source:continuous:9': .5,
            switch_ids[0]: 1,
        }
        import_path = work / 'settings-valve-import.json'
        write_fixture(import_path, json.dumps(imported, indent=2).encode())
        import_result = child.api('/settings/import', {'path': str(import_path), 'organ_id': str(project_path)})
        assert set(import_result['project_wind_controls']) == set(continuous_ids)
        wait_for(lambda value: abs(raw(value, continuous_ids[0]) - .7) < .001
                 and abs(raw(value, continuous_ids[1]) - .9) < .001
                 and raw(value, switch_ids[0]) < .001, 'settings-import-live-values')
        imported_targets = child.api('/project/wind/controls')['values']
        assert abs(imported_targets[continuous_ids[0]] - .7) < .00001
        assert abs(imported_targets[continuous_ids[1]] - .9) < .00001
        child.api('/stops/set', {'ids': ['project_101', 'project_202']})
        child.api('/notes/69/on?velocity=100', {})
        wait_for(lambda value: abs(ratios(value)[0] - loaded_ratio(.7)) < .0001
                 and abs(ratios(value)[1] - 1) < .0001, 'settings-import-live-pressure')
        stop_notes()
        child.api('/settings/save', {})
        child.api('/load_organ', {'path': str(project_path)})
        quiet()
        wait_for(lambda value: len(value['controls']) == 4
                 and abs(raw(value, continuous_ids[0]) - .7) < .001
                 and abs(raw(value, continuous_ids[1]) - .9) < .001, 'settings-import-after-save-reload')
        check('valid imported source-local openings update live audio and survive save plus reload; stale and linked saved values are discarded')

        # Save a non-default continuous setting, unload everything, then reload.
        set_control(continuous_ids[0], .41)
        wait_for(lambda value: abs(raw(value, continuous_ids[0]) - .41) < .001, 'before-save')
        child.api('/settings/save', {})
        child.api('/load_organ', {'path': str(ordinary)})
        cleared = wait_for(lambda value: not value['nodes'] and not value['controls']
                           and not value['pressure_ratios'] and not value['control_values']
                           and not value['valve_openings'], 'ordinary-clears-controls')
        assert cleared['pipe_count'] == 0
        check('loading an ordinary organ clears all source valves, controller values and wind diagnostics')
        child.api('/load_organ', {'path': str(project_path)})
        quiet()
        wait_for(lambda value: len(value['controls']) == 4
                 and abs(raw(value, continuous_ids[0]) - .41) < .001, 'reload-restores-continuous')
        child.stop()
        child = None
        child = Child(exe, profile, work / 'child-2.log')
        result['processes'].append(child.process.pid)
        child.api('/load_organ', {'path': str(project_path)})
        quiet()
        restarted = wait_for(lambda value: len(value['controls']) == 4
                             and abs(raw(value, continuous_ids[0]) - .41) < .001, 'restart-restores-continuous')
        assert restarted['nodes'] == initial['nodes'] and restarted['controls'] == initial['controls']
        child.api('/stops/set', {'ids': ['project_101', 'project_202']})
        child.api('/notes/69/on?velocity=100', {})
        wait_for(lambda value: abs(ratios(value)[0] - loaded_ratio(.41)) < .0001
                 and abs(ratios(value)[1] - 1) < .0001,
                 'restart-regulated-pressure')
        stop_notes()
        check('saved per-source continuous opening survives unload, reload and a complete process restart')
        assert {str(file): digest(file) for file in paths[0].parent.rglob('*')
                if file.is_file() and not file.name.startswith('.jm-settings')} == originals
        check('source ODFs, package definitions and sample bytes remain unchanged')
        child.stop()
        child = None
        check('both owned headless processes stopped without opening user windows')
        result['status'] = 'passed'
    except BaseException as error:
        result['status'], result['error'] = 'failed', str(error)
        raise
    finally:
        if child is not None:
            child.stop()
        result['count'] = len(result['checks'])
        (work / 'valve-result.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
        lock.unlink()
        print(json.dumps({'status': result['status'], 'count': result['count'],
                          'processes': result['processes']}, indent=2), flush=True)


if __name__ == '__main__':
    main()
