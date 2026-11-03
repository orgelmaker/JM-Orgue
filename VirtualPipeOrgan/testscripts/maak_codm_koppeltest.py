"""Maakt een eigen CODM-proeforgel om koppels, standaard-aan en pistons te meten.

Geen commerciele samples: elke noot is een zelf gesynthetiseerde sinus. Elk klavier
heeft een eigen kleur (extra harmonische), zodat in een opname te zien is WELK klavier
op WELKE noot klinkt:
  Pedal (Ped)  : grondtoon + 3e harmonische
  Great (Gt)   : grondtoon + 5e harmonische
  Swell (Sw)   : grondtoon + 7e harmonische

Gebruik:  python maak_codm_koppeltest.py <doelmap>
Resultaat:
  <doelmap>/OrganDefinitions/Koppel.CustomOrgan_Hauptwerk_xml   gewone koppels
  <doelmap>/OrganDefinitions/Schakel.CustomOrgan_Hauptwerk_xml  DefaultToEngaged, StoreInGeneral=N, pistons
"""
import math, os, struct, sys

RATE = 48000
EERSTE, LAATSTE = 36, 96
HARM = {'Ped': 3, 'Gt': 5, 'Sw': 7}


def schrijf_wav(pad, samples, unity, loops, cues):
    os.makedirs(os.path.dirname(pad), exist_ok=True)
    data = b''.join(struct.pack('<h', max(-32768, min(32767, int(s * 32767)))) for s in samples)
    chunks = [b'fmt ' + struct.pack('<IHHIIHH', 16, 1, 1, RATE, RATE * 2, 2, 16)]
    smpl = struct.pack('<9I', 0, 0, 20833, unity, 0, 0, 0, len(loops), 0)
    for i, (a, e) in enumerate(loops):
        smpl += struct.pack('<6I', i, 0, a, e, 0, 0)
    chunks.append(b'smpl' + struct.pack('<I', len(smpl)) + smpl)
    cue = struct.pack('<I', len(cues))
    for i, c in enumerate(cues):
        cue += struct.pack('<II4sIII', i + 1, c, b'data', 0, 0, c)
    chunks.append(b'cue ' + struct.pack('<I', len(cue)) + cue)
    chunks.append(b'data' + struct.pack('<I', len(data)) + data)
    body = b'WAVE' + b''.join(chunks)
    open(pad, 'wb').write(b'RIFF' + struct.pack('<I', len(body)) + body)


def noot_wav(map_, naam, noot, harm):
    f = 440.0 * 2 ** ((noot - 69) / 12.0)
    n = int(1.0 * RATE)
    s = [0.35 * math.sin(2 * math.pi * f * i / RATE) + 0.25 * math.sin(2 * math.pi * harm * f * i / RATE) for i in range(n)]
    schrijf_wav(os.path.join(map_, naam), s, noot, [(4800, 43200)], [44000])


RANK = ('<Rank><RankID>{id}</RankID><Name>{naam}</Name><NumberOfPipes>61</NumberOfPipes>'
        '<MIDINoteNumberOfFirstPipe>36</MIDINoteNumberOfFirstPipe><IsPercussive>N</IsPercussive>'
        '<Samples_InstallationPackageID>9</Samples_InstallationPackageID><Samples_MainSampleFileFolder>{map}</Samples_MainSampleFileFolder>'
        '<Samples_MIDINoteNumIncrementFromPipesToSamples>0</Samples_MIDINoteNumIncrementFromPipesToSamples>'
        '<Pitch_RankBaseOutputPitch64ftHarmonicNum>8</Pitch_RankBaseOutputPitch64ftHarmonicNum></Rank>')
STOPRANK = ('<StopRank><StopCode>{code}</StopCode><RankTypeCode>1</RankTypeCode><RankID>{id}</RankID>'
            '<MIDINoteNumIncrementFromDivisionToRank>0</MIDINoteNumIncrementFromDivisionToRank>'
            '<MIDINoteNumOfFirstMappedDivisionKey>36</MIDINoteNumOfFirstMappedDivisionKey><NumberOfMappedDivisionKeys>61</NumberOfMappedDivisionKeys>'
            '<ActionTypeCode>1</ActionTypeCode><ActionEffectCode>1</ActionEffectCode></StopRank>')


def koppel(code, naam, extra=''):
    return f'<Coupler><CouplerCode>{code}</CouplerCode><Name>Coupler: {naam}</Name>{extra}</Coupler>'


def orgel(naam, koppels, stop_extra='', pistons=''):
    stops = ''.join(f'<Stop><StopCode>{c}</StopCode><Name>{n}</Name>{stop_extra if c == 2201 else ""}</Stop>'
                    for c, n in ((2001, "Ped: Bourdon 8'"), (2101, "Gt: Principal 8'"), (2201, "Sw: Gamba 8'")))
    return f'''<?xml version="1.0" encoding="UTF-8"?>
<Hauptwerk FileFormat="CustomOrgan" FileFormatVersion="3.00">
<ObjectList ObjectType="_general"><_general><Name>{naam}</Name></_general></ObjectList>
<ObjectList ObjectType="requiredinstallationpackage"><RequiredInstallationPackage><InstallationPackageID>9</InstallationPackageID><Name>Eigen synthese</Name></RequiredInstallationPackage></ObjectList>
<ObjectList ObjectType="division">
<Division><DivisionCode>01</DivisionCode><Name>PEDAL</Name><ShortName>Ped</ShortName><NumberOfKeys>61</NumberOfKeys><MIDINoteNumOfFirstKey>36</MIDINoteNumOfFirstKey></Division>
<Division><DivisionCode>02</DivisionCode><Name>GREAT</Name><ShortName>Gt</ShortName><NumberOfKeys>61</NumberOfKeys><MIDINoteNumOfFirstKey>36</MIDINoteNumOfFirstKey></Division>
<Division><DivisionCode>03</DivisionCode><Name>SWELL</Name><ShortName>Sw</ShortName><NumberOfKeys>61</NumberOfKeys><MIDINoteNumOfFirstKey>36</MIDINoteNumOfFirstKey></Division>
</ObjectList>
<ObjectList ObjectType="coupler">{koppels}</ObjectList>
<ObjectList ObjectType="rank">{RANK.format(id=1, naam="Ped", map="Ped")}{RANK.format(id=2, naam="Gt", map="Gt")}{RANK.format(id=3, naam="Sw", map="Sw")}</ObjectList>
<ObjectList ObjectType="stop">{stops}</ObjectList>
<ObjectList ObjectType="stoprank">{STOPRANK.format(code=2001, id=1)}{STOPRANK.format(code=2101, id=2)}{STOPRANK.format(code=2201, id=3)}</ObjectList>
{pistons}</Hauptwerk>
'''


def main(doel):
    pak = os.path.join(doel, 'OrganInstallationPackages', '000009')
    os.makedirs(pak, exist_ok=True)
    open(os.path.join(pak, 'PackageID000009.InstallationPackageDefinition_Hauptwerk_xml'), 'w').write(
        '<?xml version="1.0"?><Hauptwerk FileFormat="InstallationPackage" FileFormatVersion="4.0"/>')
    for noot in range(EERSTE, LAATSTE + 1):
        for map_, k in (('Ped', 'Ped'), ('Gt', 'Gt'), ('Sw', 'Sw')):
            noot_wav(os.path.join(pak, map_), f'{noot:03}-x.wav', noot, HARM[k])
    defmap = os.path.join(doel, 'OrganDefinitions')
    os.makedirs(defmap, exist_ok=True)
    # Koppelcodes: 1000 + 100*doel(toetsen) + 5*bron(klinkt) + soort; divisie-index 0=Ped, 1=Gt, 2=Sw.
    koppels = ''.join([
        koppel(1110, "Sw to Gt 16'"), koppel(1111, "Sw to Gt 8'"), koppel(1112, "Sw to Gt 4'"),
        koppel(1106, "Gt Unison Off"), koppel(1006, "Gt to Ped 8'"),
        koppel(1103, "Bass"), koppel(1114, "Swell Melody"),
    ])
    open(os.path.join(defmap, 'Koppel.CustomOrgan_Hauptwerk_xml'), 'w', encoding='utf-8').write(orgel('Koppel-proef', koppels))
    # Tweede orgel: standaard aan, buiten de combinaties, pistons.
    koppels2 = ''.join([
        koppel(1111, "Sw to Gt 8'"),
        koppel(1112, "Sw to Gt 4'", '<DefaultToEngaged>Y</DefaultToEngaged><StoreInGeneralCombinations>N</StoreInGeneralCombinations>'),
    ])
    pistons = ('<ObjectList ObjectType="shortcutpiston">'
               '<ShortcutPiston><Name>Rev 8</Name><ReferencedObjectTypeCode>2</ReferencedObjectTypeCode><CouplerCode>1111</CouplerCode><PistonActionTypeCode>1</PistonActionTypeCode><CustomDisplay1_Text>S/G</CustomDisplay1_Text></ShortcutPiston>'
               '<ShortcutPiston><Name>Hold 4</Name><ReferencedObjectTypeCode>2</ReferencedObjectTypeCode><CouplerCode>1112</CouplerCode><PistonActionTypeCode>2</PistonActionTypeCode></ShortcutPiston>'
               '<ShortcutPiston><Name>Stop Sw</Name><ReferencedObjectTypeCode>1</ReferencedObjectTypeCode><StopCode>2201</StopCode><PistonActionTypeCode>1</PistonActionTypeCode></ShortcutPiston>'
               '</ObjectList>\n')
    open(os.path.join(defmap, 'Schakel.CustomOrgan_Hauptwerk_xml'), 'w', encoding='utf-8').write(
        orgel('Schakel-proef', koppels2, '<DefaultToEngaged>Y</DefaultToEngaged>', pistons))
    print('klaar:', defmap)


if __name__ == '__main__':
    main(sys.argv[1] if len(sys.argv) > 1 else 'codm-koppeltest')
