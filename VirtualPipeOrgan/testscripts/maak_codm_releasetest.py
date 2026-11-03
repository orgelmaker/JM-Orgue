"""Maakt een kleine, volledig eigen CODM-proeforgel om releases en articulatie te meten.

Geen commerciele samples: alle WAV's zijn zelf gesynthetiseerd (sinus).
Gebruik:  python maak_codm_releasetest.py <doelmap>
Resultaat: <doelmap>/OrganDefinitions/Mini.CustomOrgan_Hauptwerk_xml  (laden in de app)

Twee registers, elk met twee pijpen (noot 60/61):
  Gt: Gescheiden  - aparte releases: kort (<=150 ms, 600 Hz), middel (<=300 ms, 900 Hz),
                    primair (1300 Hz, 0,9 s); hoofdbestand 220 Hz met loop.
  Gt: MeerCues    - release in het hoofdbestand met TWEE cue-markers: de eerste (0,5 s)
                    begint een 700 Hz-release, de tweede (0,8 s) een 1400 Hz-staart.
                    Hauptwerk/CODM: release start bij de EERSTE marker (700 Hz).
"""
import math, os, struct, sys

RATE = 48000


def sinus(freq, secs, amp=0.5, fade_out=False, start=0.0):
    n = int(secs * RATE)
    uit = []
    for i in range(n):
        env = 1.0
        if fade_out:
            env = max(0.0, 1.0 - i / n) ** 1.5
        uit.append(amp * env * math.sin(2 * math.pi * freq * (i / RATE + start)))
    return uit


def schrijf_wav(pad, samples, loops=(), cues=()):
    os.makedirs(os.path.dirname(pad), exist_ok=True)
    data = b''.join(struct.pack('<h', max(-32768, min(32767, int(s * 32767)))) for s in samples)
    chunks = [b'fmt ' + struct.pack('<IHHIIHH', 16, 1, 1, RATE, RATE * 2, 2, 16)]
    if loops:
        smpl = struct.pack('<9I', 0, 0, 20833, 60, 0, 0, 0, len(loops), 0)
        for i, (a, e) in enumerate(loops):
            smpl += struct.pack('<6I', i, 0, a, e, 0, 0)
        chunks.append(b'smpl' + struct.pack('<I', len(smpl)) + smpl)
    if cues:
        cue = struct.pack('<I', len(cues))
        for i, c in enumerate(cues):
            cue += struct.pack('<II4sIII', i + 1, c, b'data', 0, 0, c)
        chunks.append(b'cue ' + struct.pack('<I', len(cue)) + cue)
    chunks.append(b'data' + struct.pack('<I', len(data)) + data)
    body = b'WAVE' + b''.join(chunks)
    open(pad, 'wb').write(b'RIFF' + struct.pack('<I', len(body)) + body)


def main(doel):
    pak = os.path.join(doel, 'OrganInstallationPackages', '000009')
    os.makedirs(pak, exist_ok=True)
    open(os.path.join(pak, 'PackageID000009.InstallationPackageDefinition_Hauptwerk_xml'), 'w').write(
        '<?xml version="1.0"?><Hauptwerk FileFormat="InstallationPackage" FileFormatVersion="4.0"/>')
    for noot, naam in ((60, '060-C.wav'), (61, '061-C#.wav')):
        # Gescheiden: hoofdbestand met loop, drie losse releases.
        schrijf_wav(os.path.join(pak, 'Gesch', naam), sinus(220, 1.0), loops=[(4800, 43200)])
        schrijf_wav(os.path.join(pak, 'Gesch', 'rel150', naam), sinus(600, 0.15, fade_out=True))
        schrijf_wav(os.path.join(pak, 'Gesch', 'rel300', naam), sinus(900, 0.30, fade_out=True))
        schrijf_wav(os.path.join(pak, 'Gesch', 'rel900', naam), sinus(1300, 0.90, fade_out=True))
        # MeerCues: 220 Hz sustain tot 0,5 s, dan 700 Hz (tot 0,8 s), dan 1400 Hz (tot 1,1 s).
        s = sinus(220, 0.5) + sinus(700, 0.3, fade_out=True) + sinus(1400, 0.3, fade_out=True)
        schrijf_wav(os.path.join(pak, 'Cues', naam), s, loops=[(4800, 19200)], cues=[24000, 38400])
    xml = '''<?xml version="1.0" encoding="UTF-8"?>
<Hauptwerk FileFormat="CustomOrgan" FileFormatVersion="3.00">
<ObjectList ObjectType="_general"><_general><Name>Release-proef</Name></_general></ObjectList>
<ObjectList ObjectType="requiredinstallationpackage"><RequiredInstallationPackage><InstallationPackageID>9</InstallationPackageID><Name>Eigen synthese</Name></RequiredInstallationPackage></ObjectList>
<ObjectList ObjectType="division"><Division><DivisionCode>02</DivisionCode><Name>GREAT</Name><ShortName>Gt</ShortName><NumberOfKeys>61</NumberOfKeys><MIDINoteNumOfFirstKey>36</MIDINoteNumOfFirstKey></Division></ObjectList>
<ObjectList ObjectType="rank">
<Rank><RankID>1</RankID><Name>Gescheiden</Name><NumberOfPipes>2</NumberOfPipes><MIDINoteNumberOfFirstPipe>60</MIDINoteNumberOfFirstPipe><IsPercussive>N</IsPercussive><Samples_InstallationPackageID>9</Samples_InstallationPackageID><Samples_MainSampleFileFolder>Gesch</Samples_MainSampleFileFolder><Samples_MIDINoteNumIncrementFromPipesToSamples>0</Samples_MIDINoteNumIncrementFromPipesToSamples><Pitch_RankBaseOutputPitch64ftHarmonicNum>8</Pitch_RankBaseOutputPitch64ftHarmonicNum><Samples_ShortNoteReleaseSampleFileFolderIfAny>Gesch\\rel150</Samples_ShortNoteReleaseSampleFileFolderIfAny><Samples_MaxNoteTimeMsForShortNoteReleaseSamplesIfAny>150</Samples_MaxNoteTimeMsForShortNoteReleaseSamplesIfAny><Samples_MediumNoteReleaseSampleFileFolderIfAny>Gesch\\rel300</Samples_MediumNoteReleaseSampleFileFolderIfAny><Samples_MaxNoteTimeMsForMediumNoteReleaseSamplesIfAny>300</Samples_MaxNoteTimeMsForMediumNoteReleaseSamplesIfAny><Samples_RelSampleFileFolderIfRelSamplesStoredInSeparateFiles>Gesch\\rel900</Samples_RelSampleFileFolderIfRelSamplesStoredInSeparateFiles></Rank>
<Rank><RankID>2</RankID><Name>MeerCues</Name><NumberOfPipes>2</NumberOfPipes><MIDINoteNumberOfFirstPipe>60</MIDINoteNumberOfFirstPipe><IsPercussive>N</IsPercussive><Samples_InstallationPackageID>9</Samples_InstallationPackageID><Samples_MainSampleFileFolder>Cues</Samples_MainSampleFileFolder><Samples_MIDINoteNumIncrementFromPipesToSamples>0</Samples_MIDINoteNumIncrementFromPipesToSamples><Pitch_RankBaseOutputPitch64ftHarmonicNum>8</Pitch_RankBaseOutputPitch64ftHarmonicNum></Rank>
</ObjectList>
<ObjectList ObjectType="stop"><Stop><StopCode>2101</StopCode><Name>Gt: Gescheiden 8'</Name></Stop><Stop><StopCode>2102</StopCode><Name>Gt: MeerCues 8'</Name></Stop></ObjectList>
<ObjectList ObjectType="stoprank">
<StopRank><StopCode>2101</StopCode><RankTypeCode>1</RankTypeCode><RankID>1</RankID><MIDINoteNumIncrementFromDivisionToRank>0</MIDINoteNumIncrementFromDivisionToRank><MIDINoteNumOfFirstMappedDivisionKey>60</MIDINoteNumOfFirstMappedDivisionKey><NumberOfMappedDivisionKeys>2</NumberOfMappedDivisionKeys><ActionTypeCode>1</ActionTypeCode><ActionEffectCode>1</ActionEffectCode></StopRank>
<StopRank><StopCode>2102</StopCode><RankTypeCode>1</RankTypeCode><RankID>2</RankID><MIDINoteNumIncrementFromDivisionToRank>0</MIDINoteNumIncrementFromDivisionToRank><MIDINoteNumOfFirstMappedDivisionKey>60</MIDINoteNumOfFirstMappedDivisionKey><NumberOfMappedDivisionKeys>2</NumberOfMappedDivisionKeys><ActionTypeCode>1</ActionTypeCode><ActionEffectCode>1</ActionEffectCode></StopRank>
</ObjectList>
</Hauptwerk>
'''
    defmap = os.path.join(doel, 'OrganDefinitions')
    os.makedirs(defmap, exist_ok=True)
    pad = os.path.join(defmap, 'Mini.CustomOrgan_Hauptwerk_xml')
    open(pad, 'w', encoding='utf-8').write(xml)
    print('klaar:', pad)


if __name__ == '__main__':
    main(sys.argv[1] if len(sys.argv) > 1 else 'codm-releasetest')
