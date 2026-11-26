"""Measure source-local bellows feedback and timed automation without windows.

These synthetic ODFs exercise JM-Orgue's declared control/pneumatic model. They
are not a claim that Hauptwerk's undisclosed numerical implementation is equal.
Each source has a bellows-extension loop, a pressure loop, and a separate
Willis/inertia cascade plus a stage switch. Raw ODF IDs intentionally overlap.
"""
import argparse
import copy
import json
import math
import os
from pathlib import Path
import statistics
import sys
import time
import uuid
from urllib.error import HTTPError
from urllib.request import Request, urlopen
from xml.etree.ElementTree import Element, SubElement, fromstring, tostring

from console_texture_background_test import Child, digest
from organ_project_background_test import make_wave, organ_text, write_fixture


PURPOSE = 'jm-orgue-project-feedback-background-test'
ALLOWED_DIR = 'orgelbouwer-feedback-2026-11-26'
SOURCE_IDS = [str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE + '/' + letter)) for letter in ('A', 'B')]
PROJECT_ID = str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE))
CONTROL_NUMBERS = (10, 11, 20, 21, 22, 30, 31)


def xml(letter):
    document = Element('Hauptwerk', FileFormat='Organ', FileFormatVersion='4.0')

    def table(kind, rows):
        objects = SubElement(document, 'ObjectList', ObjectType=kind)
        for values in rows:
            row = SubElement(objects, kind)
            for field, value in values.items():
                SubElement(row, field).text = str(value)

    table('_General', [{'Identification_Name': 'Feedbackbron ' + letter,
                        'EnablePlayingAtOriginalOrganPitch': 'Y'}])
    table('Division', [{'DivisionID': 1, 'Name': 'Bronmanual'}])
    table('Stop', [{'StopID': rank, 'Name': name, 'DivisionID': 1}
                   for rank, name in ((1, 'Balgtest 8'), (2, 'Druktest 8'), (3, 'Tijdtest 8'))])
    table('StopRank', [{'StopID': rank, 'RankID': rank,
                       'MIDINoteNumOfFirstMappedDivisionInputNode': 69,
                       'NumberOfMappedDivisionInputNodes': 1} for rank in (1, 2, 3)])
    table('Rank', [{'RankID': rank, 'Name': 'Teststem ' + str(rank)} for rank in (1, 2, 3)])
    table('Pipe_SoundEngine01', [{
        'PipeID': rank, 'RankID': rank, 'NormalMIDINoteNumber': 69,
        'Pitch_Tempered_RankBasePitch64ftHarmonicNum': 8,
        'Pitch_OriginalOrgan_SpecificationMethodCode': 2, 'Pitch_OriginalOrgan_PitchHz': 440,
        'WindSupply_SourceWindCompartmentID': rank + 1,
        'WindSupply_MassFlowRateKilogramsPerSecAtReferencePressureDiff': .008,
        'WindSupply_ReferencePressureDifferenceInches': 4,
    } for rank in (1, 2, 3)])
    table('Pipe_SoundEngine01_Layer', [{'LayerID': rank, 'PipeID': rank, 'PipeLayerNumber': 1}
                                     for rank in (1, 2, 3)])
    table('Pipe_SoundEngine01_AttackSample', [{'UniqueID': rank, 'LayerID': rank, 'SampleID': 1}
                                            for rank in (1, 2, 3)])
    table('Sample', [{'SampleID': 1, 'InstallationPackageID': 1, 'SampleFilename': 'tone.wav',
                      'Pitch_SpecificationMethodCode': 4, 'Pitch_ExactSamplePitch': 440}])
    table('Switch', [{'SwitchID': 7, 'Name': 'Eindstand 127', 'DefaultToEngaged': 'N'}])
    defaults = {10: 64, 11: 127, 20: 0, 21: 0, 22: 0, 30: 0, 31: 0}
    table('ContinuousControl', [{'ControlID': number, 'Name': 'Regelaar ' + str(number),
                                 'DefaultValue': defaults[number]} for number in CONTROL_NUMBERS])
    table('WindCompartment', [
        {'WindCompartmentID': 1, 'Name': 'Ventilator', 'InfiniteVolume': 'Y', 'DefaultAirPressureInches': 6},
        {'WindCompartmentID': 2, 'Name': 'Gewichtsbalg', 'InfiniteVolume': 'N',
         'StandardVolumeMetresCubed': .08, 'DefaultAirPressureInches': 4,
         'Bellows_HasBellows': 'Y', 'Bellows_FrameBaseWidthMetres': .5,
         'Bellows_FrameBaseLengthMetres': .5, 'Bellows_MaximumExtensionMetres': .1,
         'Bellows_MassOfMovingBoardActedOnByGravityKg': 4 * 249.089 * .25 / 9.81,
         'Bellows_MassOfMovingBoardGivingRiseToInertiaKg': 4 * 249.089 * .25 / 9.81,
         'Bellows_PositiveDampingCoefficient': 4000,
         'Bellows_ExtensionOutputContinuousControlID': 10},
        {'WindCompartmentID': 3, 'Name': 'Drukgeregelde lade', 'InfiniteVolume': 'N',
         'StandardVolumeMetresCubed': .8, 'DefaultAirPressureInches': 4,
         'PressureOutputContinuousControlID': 11},
        {'WindCompartmentID': 4, 'Name': 'Tijdgeregelde lade', 'InfiniteVolume': 'N',
         'StandardVolumeMetresCubed': .08, 'DefaultAirPressureInches': 6},
        {'WindCompartmentID': 5, 'Name': 'Eindstandindicator', 'InfiniteVolume': 'N',
         'StandardVolumeMetresCubed': .08, 'DefaultAirPressureInches': 6},
    ])
    links = []
    for destination, controller in ((2, 30), (3, 31), (4, 22)):
        links.append({'FirstWindCompartmentID': 1, 'SecondWindCompartmentID': destination,
                      'MassFlowRateKilogramsPerSecAtReferencePressureDiff': .02,
                      'ReferencePressureDifferenceInches': 1, 'ValveControlTypeCode': 3,
                      'ValveControllingContinuousControlID': controller})
    links.append({'FirstWindCompartmentID': 1, 'SecondWindCompartmentID': 5,
                  'MassFlowRateKilogramsPerSecAtReferencePressureDiff': .02,
                  'ReferencePressureDifferenceInches': 1, 'ValveControlTypeCode': 2,
                  'ValveControllingSwitchID': 7, 'ValveOpenIfControllingSwitchEngaged': 'Y'})
    table('WindCompartmentLinkage', links)
    transfers = []
    for source, destination, model, inverted, increment, coefficient, extra in (
        (10, 30, 1, 'Y', -63, 2, {}),
        (11, 31, 1, 'Y', 0, 8, {}),
        (20, 21, 2, 'N', 0, 1, {'WillisTypeIncSpeedInMillisecondsPerStepWithOneStepDiff': 250}),
        (21, 22, 3, 'N', 0, 1, {'InertiaModelTypePositiveAcceleratingCoeff': 16,
                               'InertiaModelTypePositiveDampingCoeff': 8}),
    ):
        transfers.append({'SourceControlID': source, 'DestControlID': destination,
                          'LinkTypeCode': model, 'InvertSourceControlValue': inverted,
                          'SourceControlValueIncrement': increment,
                          'SourceControlValueCoefficient': coefficient,
                          'SourceControlValueIndex': 1, **extra})
    table('ContinuousControlLinkage', transfers)
    table('ContinuousControlStageSwitch', [{
        'ContinuousControlID': 22, 'ContinuousControlValue': 127, 'ControlledSwitchID': 7,
        'EngageWhenValueIncreasing': 'Y', 'EngageWhenValueDecreasing': 'N',
        'DisengageWhenValueIncreasing': 'N', 'DisengageWhenValueDecreasing': 'Y',
    }])
    return tostring(document, encoding='utf-8', xml_declaration=True)


def prepare(work):
    sources = work / 'sources'
    sources.mkdir(exist_ok=True)
    paths, roots = [], []
    for letter in ('A', 'B'):
        root = sources / ('packages-' + letter)
        package = root / 'OrganInstallationPackages' / '000001'
        package.mkdir(parents=True, exist_ok=True)
        make_wave(package / 'tone.wav', 48000, 440, False)
        write_fixture(package / 'PackageID000001.InstallationPackageDefinition_Hauptwerk_xml', b'<Hauptwerk/>')
        path = sources / ('Feedback ' + letter + '.Organ_Hauptwerk_xml')
        write_fixture(path, xml(letter))
        paths.append(path)
        roots.append(root)
    ordinary = sources / 'Gewone controle.organ'
    write_fixture(ordinary, organ_text('gewoon', ['Grondstem 8'], ['packages-A/OrganInstallationPackages/000001/tone.wav']))
    missing_compartments = sources / 'Ontbrekende windcompartimenten.Organ_Hauptwerk_xml'
    malformed = fromstring(xml('A'))
    for table in list(malformed):
        if table.get('ObjectType') == 'WindCompartment':
            malformed.remove(table)
    write_fixture(missing_compartments, tostring(malformed, encoding='utf-8', xml_declaration=True))
    originals = {str(file): digest(file) for file in sources.rglob('*') if file.is_file()}
    return paths, roots, ordinary, missing_compartments, originals


def source_fingerprint(path):
    """Use the public project's raw-file fingerprint when inspection rejects early."""
    data = path.read_bytes()
    hashed = 0xcbf29ce484222325
    for byte in data:
        hashed = ((hashed ^ byte) * 0x100000001b3) & 0xffffffffffffffff
    return f'fnv1a64:{hashed:016x}:{len(data)}'


def is_missing_compartment_error(value):
    message = str(value).lower()
    return ('windverbinding' in message and 'ontbrekende of gelijke begin-/eindlade' in message
            or 'wind' in message and 'compartiment' in message and 'ontbre' in message)


def pressure_equilibrium():
    # .008 sqrt(p) pipe flow against .02 * 8(1-p) * sqrt(6-4p)
    # supply flow; p is pressure divided by the nominal four inches.
    low, high = 0, 1
    for _ in range(80):
        mid = (low + high) / 2
        supplied = .16 * (1 - mid) * math.sqrt(6 - 4 * mid)
        demanded = .008 * math.sqrt(mid)
        if supplied > demanded:
            low = mid
        else:
            high = mid
    return (low + high) / 2


def settling_time(trace, get_value, target, tolerance):
    """First recorded model time after which all remaining samples are in band."""
    last_outside = -1
    for index, sample in enumerate(trace):
        if abs(get_value(sample) - target) > tolerance:
            last_outside = index
    return trace[last_outside + 1]['t'] if last_outside + 1 < len(trace) else None


def main():
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', required=True, type=Path)
    parser.add_argument('--work-dir', required=True, type=Path)
    parser.add_argument('--prepare-only', action='store_true')
    args = parser.parse_args()
    assert args.exe.is_absolute() and args.work_dir.is_absolute()
    exe, work = args.exe.resolve(strict=True), args.work_dir.resolve()
    allowed = Path(__file__).resolve().parents[2] / 'testen' / ALLOWED_DIR
    work.relative_to(allowed.resolve())
    assert exe.name.lower() in ('vpo-app.exe', 'vpo-app')
    work.mkdir(parents=True, exist_ok=True)
    assert not work.is_symlink()
    write_fixture(work / '.jm-project-feedback-fixture', (PURPOSE + '\n').encode())
    lock = work / '.running'
    with lock.open('x', encoding='utf-8') as file:
        file.write(str(os.getpid()))
    result = {'status': 'running', 'checks': [], 'processes': [], 'snapshots': {}, 'traces': {}, 'metrics': {},
              'model_contract': 'JM normalized sensors; transformed raw 0..127; Willis first-order tau_ms; analytical second-order inertia; prior-tick SCC feedback; bounded physical bellows.'}
    child = None
    control_slots, node_slots, link_slots = {}, {}, {}

    def check(label):
        result['checks'].append(label)
        print('OK ' + label, flush=True)

    def wind():
        return child.api('/project/wind')

    def wait_for(predicate, label, seconds=5):
        deadline = time.monotonic() + seconds
        value = None
        while time.monotonic() < deadline:
            value = wind()
            if predicate(value):
                result['snapshots'][label] = value
                return value
            time.sleep(.015)
        raise AssertionError(f'{label}: {value}')

    def control_id(source, number, kind='continuous'):
        return SOURCE_IDS[source] + ':' + kind + ':' + str(number)

    def value(snapshot, source, number, kind='continuous'):
        return snapshot['control_values'][control_slots[control_id(source, number, kind)]]

    def node(snapshot, source, number, field='pressure_ratios'):
        return snapshot[field][node_slots[(source, number)]]

    def set_input(source, opening):
        reply = child.api('/project/wind/control', {'control_id': control_id(source, 20), 'value': opening})
        assert reply.get('warning') is None, reply

    def independent_targets():
        current = child.api('/project/wind/controls')
        return {control_id(source, 20): current['values'][control_id(source, 20)] for source in (0, 1)}

    def inspect_malformed(path, package_root):
        # Only this already-verified owned child's localhost endpoint is used.
        # Inspection may reject earlier than save/load, which is also safe.
        request = Request(f'http://127.0.0.1:{child.port}/project/source',
                          data=json.dumps({'path': str(path), 'package_root': str(package_root)}).encode(),
                          headers={'Content-Type': 'application/json'})
        try:
            with urlopen(request, timeout=30) as response:
                status, reply = response.status, json.load(response)
        except HTTPError as error:
            status, reply = error.code, json.load(error)
        assert status in (200, 400), (status, reply)
        result['snapshots']['missing-compartments-inspection'] = {'status': status, 'reply': reply}
        if status == 200:
            assert reply['fingerprint'] == source_fingerprint(path), reply
            return reply['fingerprint']
        assert is_missing_compartment_error(reply), reply
        return source_fingerprint(path)

    def quiet():
        child.api('/settings/master?db=-40', {})
        child.api('/settings/wind?enabled=0', {})
        child.api('/reverb', {'mix': 0})

    def stop_notes():
        child.api('/notes/69/off', {})
        child.api('/panic', {})
        child.api('/stops/set', {'ids': []})
        time.sleep(.05)

    def capture(label, seconds):
        first = wind()
        start = first['model_time_seconds']
        deadline = time.monotonic() + seconds + 5
        trace = []
        while time.monotonic() < deadline:
            snapshot = wind()
            sample = {
                't': snapshot['model_time_seconds'] - start,
                'tick': snapshot['control_tick_count'],
                'pressure': [[node(snapshot, source, number) for number in (2, 3, 4)] for source in (0, 1)],
                'extension': [node(snapshot, source, 2, 'bellows_extensions') for source in (0, 1)],
                'position': [node(snapshot, source, 2, 'bellows_positions') for source in (0, 1)],
                'velocity': [node(snapshot, source, 2, 'bellows_velocities') for source in (0, 1)],
                'controls': [[value(snapshot, source, number) for number in CONTROL_NUMBERS] for source in (0, 1)],
                'stage': [value(snapshot, source, 7, 'switch') for source in (0, 1)],
                'valves': [[snapshot['valve_openings'][link_slots[control_id(source, number, kind)]]
                            for number, kind in ((30, 'continuous'), (31, 'continuous'), (22, 'continuous'), (7, 'switch'))]
                           for source in (0, 1)],
            }
            assert all(math.isfinite(number) for row in sample['pressure'] + sample['controls'] for number in row), sample
            assert all(0 <= number <= 1 for row in sample['controls'] for number in row), sample
            assert all(0 <= number <= 1 for number in sample['extension']), sample
            # Diagnostics are atomically published once per callback; separate
            # arrays may straddle a callback (about 3 ms). Allow that acquisition
            # skew here, and compare actuator/sensor values at settled phases.
            assert all(abs(position - extension * .1) < .0002 for position, extension in zip(sample['position'], sample['extension'])), sample
            if not trace or sample['tick'] != trace[-1]['tick']:
                trace.append(sample)
            if sample['t'] >= seconds:
                result['traces'][label] = trace
                return trace
            time.sleep(.015)
        raise AssertionError('Model clock did not advance: ' + label)

    try:
        paths, roots, ordinary, missing_compartments, originals = prepare(work)
        if args.prepare_only:
            result['status'] = 'prepared'
            check('feedback fixtures prepared without starting a process')
            return
        profile = work / 'profile'
        profile.mkdir(exist_ok=True)
        child = Child(exe, profile, work / 'child-1.log')
        result['processes'].append(child.process.pid)
        catalogs = [child.api('/project/source', {'path': str(path), 'package_root': str(root)})
                    for path, root in zip(paths, roots)]
        assert all(len(catalog['stops']) == 3 for catalog in catalogs)
        project = {'schema_version': 1, 'id': PROJECT_ID, 'name': 'Twee feedbackbronnen',
                   'target_a4_hz': 440, 'next_stop_id': 204, 'next_division_id': 2, 'sources': [],
                   'divisions': [{'id': 1, 'name': 'Gedeeld manual', 'midi_channel': 2,
                                  'first_midi_note': 69, 'key_count': 1, 'pedal': False}], 'stops': [], 'couplers': []}
        for index, (path, root, catalog, source_id) in enumerate(zip(paths, roots, catalogs, SOURCE_IDS), 1):
            project['sources'].append({'id': source_id, 'path': str(path), 'package_root': str(root),
                'fingerprint': catalog['fingerprint'], 'resources': [], 'reference_a4_hz': 440,
                'tuning_policy': 'original', 'tuning_cents': 0, 'perspective': None})
            for source_stop in (1, 2, 3):
                project['stops'].append({'id': index * 100 + source_stop, 'source_id': source_id,
                    'source_stop_id': source_stop, 'division_id': 1, 'name': None})
        project_path = work / 'Twee feedbacknetwerken.jmorgan'
        child.api('/project/save', {'path': str(project_path), 'project': project})
        child.api('/load_organ', {'path': str(project_path)})
        quiet()
        initial = wait_for(lambda snapshot: len(snapshot['controls']) == 16 and len(snapshot['nodes']) == 10
                           and len(snapshot['control_values']) == 16 and len(snapshot['pressure_ratios']) == 10
                           and len(snapshot['bellows_extensions']) == 10 and len(snapshot['valve_openings']) == 8
                           and snapshot['control_tick_count'] > 0, 'loaded')
        assert len(initial['links']) == 8 and initial['pipe_count'] == 6
        control_slots = {control['id']: index for index, control in enumerate(initial['controls'])}
        for index, link in enumerate(initial['links']):
            link_slots[initial['controls'][link['control']]['id']] = index
        for source, source_id in enumerate(SOURCE_IDS):
            for number in range(1, 6):
                node_slots[(source, number)] = next(index for index, item in enumerate(initial['nodes'])
                                                   if item['source_id'] == source_id and item['compartment_id'] == number)
            for number in CONTROL_NUMBERS:
                metadata = initial['controls'][control_slots[control_id(source, number)]]
                assert metadata['automatic'] == (number != 20), metadata
            assert initial['controls'][control_slots[control_id(source, 7, 'switch')]]['automatic']
        check('sensor and timed-control dependencies compile independently for two sources with overlapping IDs')

        startup = capture('startup', .3)
        assert all(abs(sample['extension'][source] - 64 / 127) < .015 for sample in startup for source in (0, 1))
        assert all(abs(sample['pressure'][source][0] - 1) < .01 for sample in startup for source in (0, 1))
        assert all(sample['stage'] == [0, 0] for sample in startup)
        assert startup[-1]['tick'] > startup[0]['tick']
        check('startup uses finite source sensor defaults and stable physical bellows at nominal pressure')

        before_targets = independent_targets()
        for number, kind in ((10, 'continuous'), (11, 'continuous'), (21, 'continuous'),
                             (22, 'continuous'), (30, 'continuous'), (31, 'continuous'), (7, 'switch')):
            child.api('/project/wind/control', {'control_id': control_id(0, number, kind), 'value': .8}, expected=400)
            assert independent_targets() == before_targets
        check('physical sensors, timed outputs and stage switches reject direct manual writes')

        before_bad_organ = child.api('/organ')
        before_bad_metadata = child.api('/project/wind/controls')['controls']
        before_bad_targets = independent_targets()
        before_bad_settings = child.api('/settings/organ')
        library_path = profile / 'organ_library.json'
        before_bad_settings_map = json.loads(library_path.read_text(encoding='utf-8-sig'))['settings']
        before_bad_project_hash = digest(project_path)
        bad_project = copy.deepcopy(project)
        bad_project['sources'][0].update(
            path=str(missing_compartments),
            fingerprint=inspect_malformed(missing_compartments, roots[0]),
        )

        def assert_previous_project_retained():
            assert child.api('/organ') == before_bad_organ
            assert child.api('/project/wind/controls')['controls'] == before_bad_metadata
            assert independent_targets() == before_bad_targets
            assert child.api('/settings/organ') == before_bad_settings
            assert json.loads(library_path.read_text(encoding='utf-8-sig'))['settings'] == before_bad_settings_map
            assert digest(project_path) == before_bad_project_hash

        rejected_save = child.api('/project/save', {'path': str(project_path), 'project': bad_project}, expected=400)
        assert is_missing_compartment_error(rejected_save), rejected_save
        assert_previous_project_retained()
        bad_project_path = work / 'Ontbrekende windcompartimenten.jmorgan'
        write_fixture(bad_project_path, json.dumps(bad_project, indent=2).encode())
        rejected_load = child.api('/load_organ', {'path': str(bad_project_path)}, expected=500)
        assert is_missing_compartment_error(rejected_load), rejected_load
        assert_previous_project_retained()
        check('wind links and valves without compartment definitions reject save and load before changing the current project, organ or settings')

        child.api('/stops/set', {'ids': ['project_101']})
        child.api('/notes/69/on?velocity=100', {})
        demand = capture('bellows-demand-a', 2.5)
        tail = demand[-8:]
        result['metrics']['bellows-demand-a'] = {
            'minimum_extension': min(sample['extension'][0] for sample in demand),
            'maximum_pressure_ratio': max(sample['pressure'][0][0] for sample in demand),
            'minimum_pressure_ratio': min(sample['pressure'][0][0] for sample in demand),
            'final_extension': statistics.mean(sample['extension'][0] for sample in tail),
            'final_pressure_ratio': statistics.mean(sample['pressure'][0][0] for sample in tail),
        }
        assert min(sample['extension'][0] for sample in demand) < 64 / 127 - .04, result['metrics']
        assert all(.05 < sample['extension'][0] < .95 for sample in tail), result['metrics']
        assert all(abs(sample['pressure'][0][0] - 1) < .05 for sample in tail), result['metrics']
        assert all(abs(sample['controls'][0][0] - sample['extension'][0]) < .01 for sample in tail)
        assert all(abs(sample['valves'][0][0] - sample['controls'][0][5]) < .005 for sample in tail)
        assert all(abs(sample['extension'][1] - 64 / 127) < .002 and abs(sample['pressure'][1][0] - 1) < .002 for sample in demand)
        stop_notes()
        recovered = capture('bellows-recovery-a', 2.5)
        result['metrics']['bellows-recovery-a'] = {
            'maximum_extension': max(sample['extension'][0] for sample in recovered),
            'extension_overshoot': max(0, max(sample['extension'][0] for sample in recovered) - 64 / 127),
            'settling_time_extension_band_0_025': settling_time(recovered, lambda sample: sample['extension'][0], 64 / 127, .025),
            'final_velocity_m_s': recovered[-1]['velocity'][0],
        }
        assert abs(recovered[-1]['extension'][0] - 64 / 127) < .025, result['metrics']
        assert abs(recovered[-1]['velocity'][0]) < .005, recovered[-1]
        check('real bellows motion changes its sensor and inlet valve under demand, then settles after release without affecting source B')

        child.api('/stops/set', {'ids': ['project_102']})
        child.api('/notes/69/on?velocity=100', {})
        pressure = capture('pressure-demand-a', .9)
        expected_pressure = pressure_equilibrium()
        result['metrics']['pressure-demand-a'] = {'expected_ratio': expected_pressure,
            'minimum_ratio': min(sample['pressure'][0][1] for sample in pressure),
            'final_ratio': pressure[-1]['pressure'][0][1],
            'settling_time_ratio_band_0_005': settling_time(pressure, lambda sample: sample['pressure'][0][1], expected_pressure, .005)}
        assert abs(pressure[-1]['pressure'][0][1] - expected_pressure) < .005, result['metrics']
        assert abs(pressure[-1]['controls'][0][1] - pressure[-1]['pressure'][0][1]) < .005
        assert abs(pressure[-1]['controls'][0][6] - 8 * (1 - pressure[-1]['pressure'][0][1])) < .02
        assert abs(pressure[-1]['valves'][0][1] - pressure[-1]['controls'][0][6]) < .005
        assert all(abs(sample['pressure'][1][1] - 1) < .0001 for sample in pressure)
        stop_notes()
        check('physical pressure feeds its source-local control loop and reaches the predicted loaded equilibrium')

        set_input(0, 1)
        up = capture('timed-step-up-a', 3.2)
        willis_errors, inertia_errors = [], []
        for sample in up:
            t = max(0, sample['t'] - .005)
            if .08 < t < 2:
                willis_errors.append(abs(sample['controls'][0][3] - (1 - math.exp(-4 * t))))
                inertia_errors.append(abs(sample['controls'][0][4] - (1 - math.exp(-4 * t) * (1 + 4 * t + 8 * t * t))))
            assert sample['controls'][1][2:5] == [0, 0, 0], sample
        result['metrics']['timed-step-up-a'] = {'max_willis_error': max(willis_errors),
            'max_inertia_error': max(inertia_errors),
            'peak_value': max(sample['controls'][0][4] for sample in up),
            'overshoot': max(0, max(sample['controls'][0][4] for sample in up) - 1),
            'settling_time_willis_2percent': settling_time(up, lambda sample: sample['controls'][0][3], 1, .02),
            'settling_time_inertia_2percent': settling_time(up, lambda sample: sample['controls'][0][4], 1, .02),
            'stage_first_on_seconds': next((sample['t'] for sample in up if sample['stage'][0] == 1), None)}
        assert max(willis_errors) < .035 and max(inertia_errors) < .025, result['metrics']
        assert .8 < result['metrics']['timed-step-up-a']['settling_time_willis_2percent'] < 1.2
        assert 1.5 < result['metrics']['timed-step-up-a']['settling_time_inertia_2percent'] < 2.5
        assert up[-1]['controls'][0][3] > .999 and up[-1]['controls'][0][4] > .995, up[-1]
        assert up[-1]['stage'] == [1, 0], up[-1]
        assert up[-1]['valves'][0][3] == 1 and up[-1]['valves'][1][3] == 0
        assert abs(up[-1]['valves'][0][2] - up[-1]['controls'][0][4]) < .003
        assert all(later['controls'][0][4] + .0001 >= earlier['controls'][0][4] for earlier, later in zip(up, up[1:])), result['metrics']
        check('Willis and critical inertia follow the declared timed step response and reach the 7-bit endpoint stage without overshoot')
        set_input(0, 0)
        down = capture('timed-step-down-a', 3.2)
        result['metrics']['timed-step-down-a'] = {
            'minimum_value': min(sample['controls'][0][4] for sample in down),
            'settling_time_2percent': settling_time(down, lambda sample: sample['controls'][0][4], 0, .02),
        }
        assert down[-1]['controls'][0][3] < .001 and down[-1]['controls'][0][4] < .005
        assert down[-1]['stage'] == [0, 0]
        assert all(later['controls'][0][4] <= earlier['controls'][0][4] + .0001 for earlier, later in zip(down, down[1:])), down[-1]
        check('the timed chain returns smoothly to zero and releases its stage switch')

        # Reverse the sources to guard against accidental global controller IDs.
        set_input(1, 1)
        reverse = capture('timed-step-up-b', 3.2)
        assert reverse[-1]['stage'] == [0, 1]
        assert all(sample['controls'][0][2] == 0
                   and abs(sample['controls'][0][3]) < .005
                   and abs(sample['controls'][0][4]) < .005 for sample in reverse)
        set_input(1, 0)
        wait_for(lambda snapshot: value(snapshot, 1, 22) < .005, 'b-chain-reset')
        check('the second source has its own timed state despite identical raw controller and stage IDs')

        exported = child.api('/settings/organ')
        write_fixture(work / 'settings-export.json', json.dumps(exported, indent=2).encode())
        before_bad_import = independent_targets()
        library = profile / 'organ_library.json'
        before_bad_import_hash = digest(library)
        invalid = copy.deepcopy(exported)
        invalid['project_wind_controls'][control_id(0, 22)] = 2
        invalid_path = work / 'settings-invalid-automatic.json'
        write_fixture(invalid_path, json.dumps(invalid, indent=2).encode())
        child.api('/settings/import', {'path': str(invalid_path), 'organ_id': str(project_path)}, expected=400)
        assert independent_targets() == before_bad_import
        assert digest(library) == before_bad_import_hash
        check('an invalid imported automatic value is rejected before changing the durable input map')
        imported = copy.deepcopy(exported)
        imported['project_wind_controls'] = {
            control_id(0, 20): .33, control_id(1, 20): .66,
            control_id(0, 10): .99, control_id(0, 22): .77,
            control_id(0, 7, 'switch'): 1, 'removed-source:continuous:20': .5,
        }
        imported_path = work / 'settings-independent-inputs.json'
        write_fixture(imported_path, json.dumps(imported, indent=2).encode())
        imported_result = child.api('/settings/import', {'path': str(imported_path), 'organ_id': str(project_path)})
        assert set(imported_result['project_wind_controls']) == {control_id(0, 20), control_id(1, 20)}
        imported_live = wind()
        assert abs(value(imported_live, 0, 10) - node(imported_live, 0, 2, 'bellows_extensions')) < .01
        assert value(imported_live, 0, 22) < .1 and value(imported_live, 0, 7, 'switch') == 0
        check('valid settings import retains independent inputs and discards sensor, derived, stage and stale stored values')
        child.api('/settings/save', {})
        saved = child.api('/settings/organ')['project_wind_controls']
        assert set(saved) == {control_id(0, 20), control_id(1, 20)}, saved
        child.stop()
        child = None
        child = Child(exe, profile, work / 'child-2.log')
        result['processes'].append(child.process.pid)
        child.api('/load_organ', {'path': str(project_path)})
        quiet()
        reloaded = wait_for(lambda snapshot: len(snapshot['controls']) == 16
                            and abs(value(snapshot, 0, 22) - .33) < .005
                            and abs(value(snapshot, 1, 22) - .66) < .005, 'restart-chain-restored', seconds=6)
        assert reloaded['controls'] == initial['controls']
        assert abs(node(reloaded, 0, 2, 'bellows_extensions') - 64 / 127) < .015
        assert abs(node(reloaded, 1, 2, 'bellows_extensions') - 64 / 127) < .015
        check('restart restores only independent input settings and reconstructs fresh physical and automatic states')
        child.api('/load_organ', {'path': str(ordinary)})
        cleared = wait_for(lambda snapshot: not snapshot['controls'] and not snapshot['nodes'], 'ordinary-clears-automation')
        assert not cleared['control_values'] and not cleared['bellows_extensions'] and not cleared['bellows_positions']
        assert not cleared['bellows_velocities'] and not cleared['valve_openings']
        check('ordinary-organ loading clears physical bellows and automatic-controller diagnostics')
        assert {str(file): digest(file) for file in paths[0].parent.rglob('*')
                if file.is_file() and not file.name.startswith('.jm-settings')} == originals
        check('both source definitions, package markers and samples retain their original hashes')
        child.stop()
        child = None
        check('both owned headless app processes stopped without opening user windows')
        result['status'] = 'passed'
    except BaseException as error:
        result['status'], result['error'] = 'failed', str(error)
        raise
    finally:
        if child is not None:
            child.stop()
        result['count'] = len(result['checks'])
        (work / 'feedback-result.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
        lock.unlink()
        print(json.dumps({'status': result['status'], 'count': result['count'], 'processes': result['processes']}, indent=2), flush=True)


if __name__ == '__main__':
    main()
