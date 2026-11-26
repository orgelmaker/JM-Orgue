"""Exercise independent imported wind graphs in owned headless app processes."""
import argparse
import json
import os
from pathlib import Path
import sys
import time
import uuid
from console_texture_background_test import Child, digest, norm
from organ_project_background_test import make_wave, organ_text, write_fixture

PURPOSE = 'jm-orgue-project-wind-background-test'


def xml(letter):
    return f'''<Hauptwerk FileFormat="Organ" FileFormatVersion="4.0">
<ObjectList ObjectType="_General"><_General><Identification_Name>Windbron {letter}</Identification_Name><EnablePlayingAtOriginalOrganPitch>Y</EnablePlayingAtOriginalOrganPitch></_General></ObjectList>
<ObjectList ObjectType="Division"><Division><DivisionID>1</DivisionID><Name>Bronmanual</Name></Division></ObjectList>
<ObjectList ObjectType="Stop"><Stop><StopID>1</StopID><Name>Grondstem 8</Name><DivisionID>1</DivisionID></Stop></ObjectList>
<ObjectList ObjectType="Switch"><Switch><SwitchID>9</SwitchID><Name>Windmotor</Name><DefaultToEngaged>Y</DefaultToEngaged></Switch></ObjectList>
<ObjectList ObjectType="StopRank"><StopRank><StopID>1</StopID><RankID>1</RankID><MIDINoteNumOfFirstMappedDivisionInputNode>69</MIDINoteNumOfFirstMappedDivisionInputNode><NumberOfMappedDivisionInputNodes>1</NumberOfMappedDivisionInputNodes></StopRank></ObjectList>
<ObjectList ObjectType="Rank"><Rank><RankID>1</RankID><Name>Grondstem 8</Name></Rank></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01"><Pipe_SoundEngine01><PipeID>1</PipeID><RankID>1</RankID><NormalMIDINoteNumber>69</NormalMIDINoteNumber><Pitch_Tempered_RankBasePitch64ftHarmonicNum>8</Pitch_Tempered_RankBasePitch64ftHarmonicNum><Pitch_OriginalOrgan_SpecificationMethodCode>2</Pitch_OriginalOrgan_SpecificationMethodCode><Pitch_OriginalOrgan_PitchHz>440</Pitch_OriginalOrgan_PitchHz><WindSupply_SourceWindCompartmentID>2</WindSupply_SourceWindCompartmentID><WindSupply_MassFlowRateKilogramsPerSecAtReferencePressureDiff>0.008</WindSupply_MassFlowRateKilogramsPerSecAtReferencePressureDiff><WindSupply_ReferencePressureDifferenceInches>4</WindSupply_ReferencePressureDifferenceInches></Pipe_SoundEngine01></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01_Layer"><Pipe_SoundEngine01_Layer><LayerID>1</LayerID><PipeID>1</PipeID><PipeLayerNumber>1</PipeLayerNumber></Pipe_SoundEngine01_Layer></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01_AttackSample"><Pipe_SoundEngine01_AttackSample><UniqueID>1</UniqueID><LayerID>1</LayerID><SampleID>1</SampleID></Pipe_SoundEngine01_AttackSample></ObjectList>
<ObjectList ObjectType="Sample"><Sample><SampleID>1</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>tone.wav</SampleFilename><Pitch_SpecificationMethodCode>4</Pitch_SpecificationMethodCode><Pitch_ExactSamplePitch>440</Pitch_ExactSamplePitch></Sample></ObjectList>
<ObjectList ObjectType="WindCompartment"><WindCompartment><WindCompartmentID>1</WindCompartmentID><Name>Ventilator</Name><InfiniteVolume>Y</InfiniteVolume><DefaultAirPressureInches>4</DefaultAirPressureInches></WindCompartment><WindCompartment><WindCompartmentID>2</WindCompartmentID><Name>Windlade</Name><InfiniteVolume>N</InfiniteVolume><StandardVolumeMetresCubed>0.08</StandardVolumeMetresCubed><DefaultAirPressureInches>4</DefaultAirPressureInches></WindCompartment></ObjectList>
<ObjectList ObjectType="WindCompartmentLinkage"><WindCompartmentLinkage><FirstWindCompartmentID>1</FirstWindCompartmentID><SecondWindCompartmentID>2</SecondWindCompartmentID><MassFlowRateKilogramsPerSecAtReferencePressureDiff>0.02</MassFlowRateKilogramsPerSecAtReferencePressureDiff><ReferencePressureDifferenceInches>1</ReferencePressureDifferenceInches><ValveControlTypeCode>{1 if letter == "A" else 2}</ValveControlTypeCode><ValveControllingSwitchID>9</ValveControllingSwitchID><ValveOpenIfControllingSwitchEngaged>Y</ValveOpenIfControllingSwitchEngaged></WindCompartmentLinkage></ObjectList>
</Hauptwerk>'''.encode('utf-8')


def main():
    if hasattr(sys.stdout, 'reconfigure'): sys.stdout.reconfigure(encoding='utf-8')
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', required=True, type=Path)
    parser.add_argument('--work-dir', required=True, type=Path)
    args = parser.parse_args()
    assert args.exe.is_absolute() and args.work_dir.is_absolute()
    exe, work = args.exe.resolve(strict=True), args.work_dir.resolve()
    allowed = Path(__file__).resolve().parents[2] / 'testen' / 'orgelbouwer-uitbreiding-2026-11-26'
    work.relative_to(allowed.resolve())
    assert exe.name.lower() in ('vpo-app.exe', 'vpo-app')
    work.mkdir(parents=True, exist_ok=True)
    marker = work / '.jm-wind-project-fixture'
    write_fixture(marker, (PURPOSE + '\n').encode())
    assert not work.is_symlink()
    lock = work / '.running'
    with lock.open('x', encoding='utf-8') as file: file.write(str(os.getpid()))
    sources_dir = work / 'sources'; sources_dir.mkdir(exist_ok=True)
    paths, roots = [], []
    for letter in ('A', 'B'):
        root = sources_dir / ('packages-' + letter)
        package = root / 'OrganInstallationPackages' / '000001'
        package.mkdir(parents=True, exist_ok=True)
        make_wave(package / 'tone.wav', 48000, 440.0, False)
        write_fixture(package / 'PackageID000001.InstallationPackageDefinition_Hauptwerk_xml', b'<Hauptwerk/>')
        odf = sources_dir / ('Wind ' + letter + '.Organ_Hauptwerk_xml')
        write_fixture(odf, xml(letter)); paths.append(odf); roots.append(root)
    ordinary = sources_dir / 'Gewone controle.organ'
    write_fixture(ordinary, organ_text('gewoon', ['Grondstem 8'], ['packages-A/OrganInstallationPackages/000001/tone.wav']))
    originals = {str(file): digest(file) for file in sources_dir.rglob('*') if file.is_file()}
    profile = work / 'profile'; profile.mkdir(exist_ok=True)
    result, child = {'status': 'running', 'checks': [], 'processes': [], 'snapshots': {}}, None
    def check(name): result['checks'].append(name); print('OK ' + name, flush=True)
    def snapshot(label):
        value = child.api('/project/wind'); result['snapshots'][label] = value; return value
    def stop_notes():
        child.api('/notes/69/off', {}); child.api('/panic', {}); child.api('/stops/set', {'ids': []})
        # Panic purges queued NoteOns in the next callback. Let that callback
        # finish before injecting the next intentional reference note.
        time.sleep(.05)
    def wait_for(predicate, label, seconds=3):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            value = child.api('/project/wind')
            if predicate(value): result['snapshots'][label] = value; return value
            time.sleep(.05)
        raise AssertionError(f'{label}: {value}')
    try:
        child = Child(exe, profile, work / 'child-1.log'); result['processes'].append(child.process.pid)
        catalogs = [child.api('/project/source', {'path': str(path), 'package_root': str(root)}) for path, root in zip(paths, roots)]
        assert not catalogs[0]['valve_controls']
        assert len(catalogs[1]['valve_controls']) == 1
        valve = catalogs[1]['valve_controls'][0]
        assert valve['kind'] == 'switch' and valve['id'] == 9 and valve['default_value'] == 1 and not valve['issues'], valve
        assert not any('statisch open' in warning for warning in catalogs[1]['warnings']), catalogs[1]['warnings']
        check('source inspection resolves the dynamic valve switch and its engaged default')
        source_ids = [str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE + '/' + letter)) for letter in ('A', 'B')]
        project = {'schema_version': 1, 'id': str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE)), 'name': 'Onafhankelijke windbronnen',
                   'target_a4_hz': 440, 'next_stop_id': 3, 'next_division_id': 2, 'sources': [],
                   'divisions': [{'id': 1, 'name': 'Gedeeld manual', 'midi_channel': 2, 'first_midi_note': 69, 'key_count': 1, 'pedal': False}],
                   'stops': [], 'couplers': []}
        for index, (path, root, catalog, source_id) in enumerate(zip(paths, roots, catalogs, source_ids), 1):
            assert len(catalog['stops']) == 1
            project['sources'].append({'id': source_id, 'path': str(path), 'package_root': str(root), 'fingerprint': catalog['fingerprint'],
                'resources': [], 'reference_a4_hz': 440, 'tuning_policy': 'original', 'tuning_cents': 0, 'perspective': None})
            project['stops'].append({'id': index, 'source_id': source_id, 'source_stop_id': 1, 'division_id': 1, 'name': 'Bron ' + str(index)})
        project_path = work / 'Twee windnetwerken.jmorgan'
        child.api('/project/save', {'path': str(project_path), 'project': project})
        child.api('/load_organ', {'path': str(project_path)})
        child.api('/settings/master?db=-40', {}); child.api('/settings/wind?enabled=0', {})
        organ = child.api('/organ'); assert len(organ['divisions']) == 1 and organ['stop_count'] == 2
        stops = {s['internal_stop_id']: s['id'] for d in organ['divisions'] for s in d['stops']}
        initial = wait_for(lambda value: len(value['pressure_ratios']) == 4, 'loaded')
        assert len(initial['nodes']) == 4 and len(initial['pressure_ratios']) == 4
        assert initial['pipe_count'] == 2
        assert not any('statisch open' in warning for warning in initial['warnings']), initial['warnings']
        assert len(initial['controls']) == 1 and initial['controls'][0]['source_control_id'] == 9
        assert initial['controls'][0]['default_value'] == 1 and not initial['controls'][0]['project_stop_ids']
        check('loaded wind diagnostics retain the independent dynamic switch and its open default')
        nodes = [next(i for i, node in enumerate(initial['nodes']) if node['source_id'] == source_id and node['compartment_id'] == 2) for source_id in source_ids]
        assert nodes[0] != nodes[1]; check('two overlapping source compartment IDs compile into independent graphs on one target manual')
        def ratios(value): return [value['pressure_ratios'][i] for i in nodes]
        for active in (0, 1):
            stop_notes(); wait_for(lambda value: min(ratios(value)) > .999, 'rest-before-' + str(active))
            child.api('/stops/set', {'ids': [stops[active + 1]]}); child.api('/notes/69/on?velocity=100', {})
            wait_for(lambda value: abs(ratios(value)[active] - 1 / 1.04) < .00001 and abs(ratios(value)[1 - active] - 1.0) < .0001, 'only-source-' + str(active))
            check(f'only source {active + 1} consumes its own network; the other source remains at nominal pressure')
        child.api('/project/wind/enabled', {'enabled': False})
        wait_for(lambda value: all(abs(p - 1.0) < .0001 for p in ratios(value)), 'disabled')
        child.api('/project/wind/enabled', {'enabled': True})
        wait_for(lambda value: abs(ratios(value)[1] - 1 / 1.04) < .00001, 'enabled-again')
        check('live source-wind disable resets pressure; re-enable responds to the still-held pipe')
        stop_notes(); wait_for(lambda value: min(ratios(value)) > .999, 'recovered')
        child.api('/settings/save', {}); child.stop(); child = None
        child = Child(exe, profile, work / 'child-2.log'); result['processes'].append(child.process.pid)
        child.api('/load_organ', {'path': str(project_path)})
        reloaded = wait_for(lambda value: len(value['pressure_ratios']) == 4, 'after-restart')
        assert reloaded['nodes'] == initial['nodes'] and reloaded['pipe_count'] == 2
        child.api('/settings/master?db=-40', {}); child.api('/stops/set', {'ids': [stops[1]]}); child.api('/notes/69/on?velocity=100', {})
        wait_for(lambda value: abs(ratios(value)[0] - 1 / 1.04) < .00001 and abs(ratios(value)[1] - 1.0) < .0001, 'restart-source-a')
        stop_notes(); check('source graph identities and independent wind behaviour survive complete process restart')
        child.api('/load_organ', {'path': str(ordinary)})
        normal = wait_for(lambda value: not value['nodes'] and not value['pressure_ratios'], 'ordinary')
        assert not normal['nodes'] and not normal['pressure_ratios'] and normal['pipe_count'] == 0
        check('loading an ordinary organ clears project wind graphs and live pressure diagnostics')
        assert {str(file): digest(file) for file in sources_dir.rglob('*') if file.is_file() and not file.name.startswith('.jm-settings')} == originals
        check('source definitions, package markers and sample bytes remain unchanged')
        child.stop(); child = None; check('both owned headless app processes stopped without user windows')
        result['status'] = 'passed'
    except BaseException as error:
        result['status'], result['error'] = 'failed', str(error); raise
    finally:
        if child is not None: child.stop()
        result['count'] = len(result['checks'])
        (work / 'wind-result.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
        lock.unlink()
        print(json.dumps({'status': result['status'], 'count': result['count'], 'processes': result['processes']}, indent=2), flush=True)


if __name__ == '__main__': main()
