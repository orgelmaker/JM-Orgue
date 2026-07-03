"""Generate JM-Orgue icon in the style of JM-Rec icon."""
from PIL import Image, ImageDraw, ImageFont
import math

def create_icon(size=256):
    img = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # Scale factor
    s = size / 256

    # === Background: rounded rectangle, dark blue-black ===
    bg_color = (18, 18, 26, 255)
    border_color = (78, 205, 196, 255)  # teal/turquoise accent like JM-Rec
    radius = int(32 * s)

    # Draw rounded rect background
    draw.rounded_rectangle(
        [int(4*s), int(4*s), int(252*s), int(252*s)],
        radius=radius,
        fill=bg_color,
        outline=border_color,
        width=int(3*s)
    )

    # === Organ pipes (same style as JM-Rec) ===
    pipe_colors = [
        (200, 200, 220),  # light silver
        (170, 170, 195),  # medium silver
        (145, 145, 170),  # darker silver
    ]

    pipe_highlight = (235, 235, 250)
    pipe_shadow = (75, 75, 100)

    # Base platform for pipes
    base_y = int(195 * s)
    base_h = int(12 * s)
    draw.rounded_rectangle(
        [int(28*s), base_y, int(175*s), base_y + base_h],
        radius=int(3*s),
        fill=(75, 75, 100),
        outline=(95, 95, 118),
        width=1
    )

    # Pipe definitions: (center_x, width, height_from_base)
    pipes = [
        (50, 18, 90),    # C
        (72, 16, 115),   # D
        (93, 18, 140),   # E (tallest)
        (114, 17, 130),  # F
        (135, 16, 108),  # G
        (155, 14, 85),   # A
    ]

    for i, (cx, w, h) in enumerate(pipes):
        cx = int(cx * s)
        w = int(w * s)
        h = int(h * s)

        pipe_top = base_y - h
        pipe_left = cx - w // 2
        pipe_right = cx + w // 2

        # Pipe body
        color_idx = i % len(pipe_colors)
        body_color = pipe_colors[color_idx]

        # Main pipe body
        draw.rectangle(
            [pipe_left, pipe_top + int(8*s), pipe_right, base_y],
            fill=body_color
        )

        # Pipe cap (rounded top)
        cap_w = w + int(4*s)
        cap_h = int(8*s)
        draw.rounded_rectangle(
            [cx - cap_w//2, pipe_top, cx + cap_w//2, pipe_top + cap_h],
            radius=int(3*s),
            fill=pipe_highlight
        )

        # Highlight stripe (left side reflection)
        stripe_w = max(int(3*s), 2)
        draw.rectangle(
            [pipe_left + int(2*s), pipe_top + cap_h, pipe_left + int(2*s) + stripe_w, base_y - int(2*s)],
            fill=pipe_highlight
        )

        # Shadow stripe (right side)
        draw.rectangle(
            [pipe_right - int(3*s), pipe_top + cap_h, pipe_right - int(1*s), base_y - int(2*s)],
            fill=pipe_shadow
        )

        # Pipe mouth (the opening slit) - small dark rectangle
        mouth_y = base_y - int(20*s)
        mouth_h = int(6*s)
        draw.rectangle(
            [pipe_left + int(2*s), mouth_y, pipe_right - int(2*s), mouth_y + mouth_h],
            fill=(30, 30, 45)
        )
        # Upper lip
        draw.rectangle(
            [pipe_left + int(1*s), mouth_y - int(2*s), pipe_right - int(1*s), mouth_y],
            fill=pipe_highlight
        )

    # === Musical note / play symbol (replacing microphone) ===
    # Draw a stylized play/speaker symbol in the teal accent color
    # Position: right side, similar to where mic is in JM-Rec

    note_color = (78, 205, 196, 255)  # teal
    note_highlight = (180, 255, 248, 255)
    note_dark = (35, 130, 125, 255)

    # Draw a speaker/sound wave icon
    # Speaker cone
    speaker_cx = int(200 * s)
    speaker_cy = int(130 * s)

    # Speaker body (rectangle)
    sp_w = int(18 * s)
    sp_h = int(28 * s)
    draw.rectangle(
        [speaker_cx - sp_w//2, speaker_cy - sp_h//2,
         speaker_cx + sp_w//2 - int(6*s), speaker_cy + sp_h//2],
        fill=note_color
    )

    # Speaker cone (triangle)
    cone_points = [
        (speaker_cx + sp_w//2 - int(6*s), speaker_cy - sp_h//2),
        (speaker_cx + sp_w//2 + int(10*s), speaker_cy - sp_h//2 - int(12*s)),
        (speaker_cx + sp_w//2 + int(10*s), speaker_cy + sp_h//2 + int(12*s)),
        (speaker_cx + sp_w//2 - int(6*s), speaker_cy + sp_h//2),
    ]
    draw.polygon(cone_points, fill=note_color)

    # Sound waves (arcs)
    for i, offset in enumerate([18, 30, 42]):
        arc_r = int(offset * s)
        arc_w = int(3 * s) if i < 2 else int(2 * s)
        alpha = 255 - i * 60
        wave_color = (*note_color[:3], alpha)

        # Draw arc as part of ellipse
        bbox = [
            speaker_cx + sp_w//2 + int(6*s) - arc_r,
            speaker_cy - arc_r,
            speaker_cx + sp_w//2 + int(6*s) + arc_r,
            speaker_cy + arc_r
        ]
        draw.arc(bbox, start=-45, end=45, fill=wave_color, width=arc_w)

    # === Green "play" indicator (replacing red "rec" dot) ===
    # Small green dot with glow effect
    dot_cx = int(210 * s)
    dot_cy = int(55 * s)
    dot_r = int(14 * s)

    # Glow
    for r in range(dot_r + int(8*s), dot_r, -1):
        alpha = int(40 * (1 - (r - dot_r) / (int(8*s))))
        glow_color = (50, 220, 100, alpha)
        draw.ellipse(
            [dot_cx - r, dot_cy - r, dot_cx + r, dot_cy + r],
            fill=glow_color
        )

    # Main dot
    draw.ellipse(
        [dot_cx - dot_r, dot_cy - dot_r, dot_cx + dot_r, dot_cy + dot_r],
        fill=(50, 200, 80, 255)
    )

    # Play triangle inside dot
    tri_s = int(8 * s)
    play_points = [
        (dot_cx - int(4*s), dot_cy - tri_s),
        (dot_cx - int(4*s), dot_cy + tri_s),
        (dot_cx + int(7*s), dot_cy),
    ]
    draw.polygon(play_points, fill=(255, 255, 255, 230))

    # Highlight on dot
    draw.ellipse(
        [dot_cx - dot_r + int(3*s), dot_cy - dot_r + int(2*s),
         dot_cx - int(2*s), dot_cy - int(2*s)],
        fill=(120, 255, 150, 80)
    )

    # === Small keyboard keys at bottom ===
    kb_y = int(210 * s)
    kb_h = int(24 * s)
    kb_x_start = int(30 * s)
    key_w = int(20 * s)
    num_keys = 8

    # White keys
    for i in range(num_keys):
        kx = kb_x_start + i * key_w
        # White key
        draw.rectangle(
            [kx, kb_y, kx + key_w - int(2*s), kb_y + kb_h],
            fill=(235, 235, 240),
            outline=(150, 150, 170),
            width=1
        )

    # Black keys
    black_positions = [0, 1, 3, 4, 5]  # positions of black keys between white keys
    for bp in black_positions:
        kx = kb_x_start + bp * key_w + int(13*s)
        bk_w = int(13 * s)
        bk_h = int(15 * s)
        draw.rectangle(
            [kx, kb_y, kx + bk_w, kb_y + bk_h],
            fill=(30, 30, 45)
        )

    return img


def create_ico(output_path):
    """Create a multi-size .ico file."""
    sizes = [16, 32, 48, 64, 128, 256]
    images = []

    for sz in sizes:
        img = create_icon(sz)
        images.append(img)

    # Save as ICO with all sizes
    images[-1].save(
        output_path,
        format='ICO',
        sizes=[(sz, sz) for sz in sizes],
        append_images=images[:-1]
    )
    print(f"Saved ICO to {output_path}")


def create_png(output_path, size=256):
    """Create a PNG preview."""
    img = create_icon(size)
    img.save(output_path, format='PNG')
    print(f"Saved PNG to {output_path}")


if __name__ == '__main__':
    # Create preview PNG first
    create_png(r'C:\Bronbestanden\JM-Orgue\jm_orgue_icon_preview.png', 256)

    # Create the ICO file
    create_ico(r'C:\Bronbestanden\JM-Orgue\VirtualPipeOrgan\src-tauri\icons\icon.ico')

    # Also create a 256x256 PNG for Tauri (needed for some platforms)
    create_png(r'C:\Bronbestanden\JM-Orgue\VirtualPipeOrgan\src-tauri\icons\icon.png', 256)

    # Create 32x32 PNG for Tauri
    create_png(r'C:\Bronbestanden\JM-Orgue\VirtualPipeOrgan\src-tauri\icons\32x32.png', 32)

    print("Done!")
