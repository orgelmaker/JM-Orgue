"""Create two tiny synthetic GrandOrgue definitions in the same test folder."""
import argparse
import math
import struct
import wave
import zlib
from pathlib import Path


def create(directory):
    directory = directory.resolve()
    marker = directory / '.jm-console-test-fixture'
    if directory.exists() and any(directory.iterdir()) and not marker.is_file():
        raise SystemExit('Refusing to write into an existing folder without the fixture marker')
    directory.mkdir(parents=True, exist_ok=True)
    marker.write_text('Synthetic JM-Orgue Consolebouwer fixtures only\n', encoding='utf-8')
    # A small self-contained raster to exercise the real custom-texture picker.
    def png_chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff)
    pixels = bytearray()
    for y in range(128):
        pixels.append(0)
        for x in range(128):
            grain = round(10 * math.sin(x * .2 + math.sin(y * .05)) + 5 * math.sin(x * .8))
            pixels += bytes((100 + grain, 65 + grain, 35 + grain))
    texture = b'\x89PNG\r\n\x1a\n' + png_chunk(b'IHDR', struct.pack('>IIBBBBB', 128, 128, 8, 2, 0, 0, 0))
    texture += png_chunk(b'IDAT', zlib.compress(bytes(pixels))) + png_chunk(b'IEND', b'')
    (directory / 'eigen-testtextuur.png').write_bytes(texture)
    rate = 44100
    with wave.open(str(directory / '060-c.wav'), 'wb') as sound:
        sound.setparams((1, 2, rate, 0, 'NONE', 'not compressed'))
        frames = []
        for n in range(rate):
            envelope = min(1, n / 441) * min(1, (rate - n) / 2205)
            sample = 6000 * envelope * math.sin(2 * math.pi * 261.625565 * n / rate)
            frames.append(struct.pack('<h', round(sample)))
        sound.writeframes(b''.join(frames))
    for letter in ('A', 'B'):
        lines = [
            '[Organ]', f'ChurchName=Console {letter}', 'OrganBuilder=JM-Orgue Consolebouwer-test',
            'NumberOfManuals=2', 'HasPedals=N', 'NumberOfWindchestGroups=2',
            'NumberOfRanks=12', 'NumberOfEnclosures=0', 'NumberOfTremulants=0',
        ]
        names = ['Prestant 8', 'Roerfluit 8', 'Octaaf 4', 'Quint 3', 'Octaaf 2', 'Trompet 8',
                 'Bourdon 8', 'Viola 8', 'Fluit 4', 'Nasard 3', 'Woudfluit 2', 'Hobo 8']
        for manual, name in ((1, 'Hoofdwerk'), (2, 'Zwelwerk')):
            lines += [f'[WindchestGroup{manual:03}]', f'Name={name}',
                      'NumberOfEnclosures=0', 'NumberOfTremulants=0', f'[Manual{manual:03}]',
                      f'Name={name}', 'NumberOfLogicalKeys=1', 'NumberOfAccessibleKeys=1',
                      'FirstAccessibleKeyMIDINoteNumber=60', 'NumberOfStops=6',
                      'NumberOfCouplers=1' if manual == 1 else 'NumberOfCouplers=0',
                      'NumberOfDivisionals=0', 'NumberOfTremulants=0']
            lines += [f'Stop{n:03}={(manual - 1) * 6 + n:03}' for n in range(1, 7)]
            if manual == 1:
                lines += ['Coupler001=001']
        for n, name in enumerate(names, 1):
            lines += [f'[Stop{n:03}]', f'Name={name}', 'NumberOfRanks=1', f'Rank001={n:03}',
                      'FirstAccessiblePipeLogicalKeyNumber=1', 'NumberOfAccessiblePipes=1',
                      f'[Rank{n:03}]', f'Name={name}', f'WindchestGroup={1 if n <= 6 else 2}',
                      'FirstMidiNoteNumber=60', 'NumberOfLogicalPipes=1', 'Percussive=N',
                      'HarmonicNumber=8', 'AmplitudeLevel=30', 'Pipe001=060-c.wav']
        lines += ['[Coupler001]', 'Name=II/I', 'DestinationManual=2', 'DestinationKeyshift=0',
                  'UnisonOff=N', 'CoupleToSubsequentUnisonIntermanualCouplers=N']
        (directory / f'Console {letter}.organ').write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(directory)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', required=True, type=Path)
    create(parser.parse_args().directory)
