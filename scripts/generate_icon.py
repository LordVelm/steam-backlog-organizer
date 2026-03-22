"""Generate Vaultkeeper icon for Steam Backlog Organizer / Vaultkeeper app."""

from PIL import Image, ImageDraw, ImageFont
import math
import os

def draw_shield(draw, cx, cy, size, fill, outline=None, outline_width=0):
    """Draw a shield/vault door shape."""
    # Shield is a rounded top with a pointed bottom
    r = size * 0.45  # radius of the top arc
    bottom_y = cy + size * 0.5
    top_y = cy - size * 0.42
    left_x = cx - r
    right_x = cx + r

    # Draw using polygon + arc
    # Top rounded part (semicircle)
    points = []
    for angle in range(180, 361):
        rad = math.radians(angle)
        x = cx + r * math.cos(rad)
        y = top_y + r + r * math.sin(rad)
        points.append((x, y))

    # Right side curve down to point
    mid_y = (top_y + r + bottom_y) / 2
    points.append((right_x, mid_y))
    points.append((cx, bottom_y))

    # Left side back up
    points.append((left_x, mid_y))

    draw.polygon(points, fill=fill, outline=outline, width=outline_width)


def draw_controller(draw, cx, cy, size, color):
    """Draw a simplified game controller icon."""
    s = size * 0.18

    # Controller body (rounded rect)
    body_w = s * 2.2
    body_h = s * 1.2
    draw.rounded_rectangle(
        [cx - body_w, cy - body_h * 0.3, cx + body_w, cy + body_h],
        radius=s * 0.5,
        fill=color,
    )

    # Left grip
    draw.ellipse(
        [cx - body_w - s * 0.3, cy - s * 0.2, cx - body_w + s * 0.8, cy + body_h + s * 0.4],
        fill=color,
    )
    # Right grip
    draw.ellipse(
        [cx + body_w - s * 0.8, cy - s * 0.2, cx + body_w + s * 0.3, cy + body_h + s * 0.4],
        fill=color,
    )

    # D-pad (left side)
    dp_x = cx - s * 1.1
    dp_y = cy + s * 0.15
    dp_s = s * 0.25
    # Horizontal bar
    draw.rectangle([dp_x - dp_s * 2, dp_y - dp_s * 0.7, dp_x + dp_s * 2, dp_y + dp_s * 0.7], fill="#1b2838")
    # Vertical bar
    draw.rectangle([dp_x - dp_s * 0.7, dp_y - dp_s * 2, dp_x + dp_s * 0.7, dp_y + dp_s * 2], fill="#1b2838")

    # Buttons (right side) - 4 small circles
    btn_x = cx + s * 1.1
    btn_y = cy + s * 0.15
    btn_r = s * 0.2
    # Top
    draw.ellipse([btn_x - btn_r, btn_y - btn_r * 3 - btn_r, btn_x + btn_r, btn_y - btn_r * 3 + btn_r], fill="#1b2838")
    # Bottom
    draw.ellipse([btn_x - btn_r, btn_y + btn_r * 1.5 - btn_r, btn_x + btn_r, btn_y + btn_r * 1.5 + btn_r], fill="#1b2838")
    # Left
    draw.ellipse([btn_x - btn_r * 3 - btn_r, btn_y - btn_r * 0.7 - btn_r, btn_x - btn_r * 3 + btn_r, btn_y - btn_r * 0.7 + btn_r], fill="#1b2838")
    # Right
    draw.ellipse([btn_x + btn_r * 1.5 - btn_r, btn_y - btn_r * 0.7 - btn_r, btn_x + btn_r * 1.5 + btn_r, btn_y - btn_r * 0.7 + btn_r], fill="#1b2838")


def draw_keyhole(draw, cx, cy, size, color):
    """Draw a keyhole shape (vault motif)."""
    s = size * 0.06
    # Circle part
    draw.ellipse([cx - s * 1.5, cy - s * 3, cx + s * 1.5, cy], fill=color)
    # Triangle/slot part
    draw.polygon([
        (cx - s * 0.8, cy - s * 0.5),
        (cx + s * 0.8, cy - s * 0.5),
        (cx + s * 0.5, cy + s * 2.5),
        (cx - s * 0.5, cy + s * 2.5),
    ], fill=color)


def generate_icon(size):
    """Generate the Vaultkeeper icon at a given size."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    cx = size / 2
    cy = size / 2

    # Background: dark circle
    margin = size * 0.04
    draw.ellipse([margin, margin, size - margin, size - margin], fill="#1b2838")

    # Shield shape - Steam blue
    shield_size = size * 0.75
    draw_shield(draw, cx, cy + size * 0.02, shield_size, fill="#66c0f4", outline="#4a9fd4", outline_width=max(1, int(size * 0.01)))

    # Inner shield - darker
    inner_size = shield_size * 0.78
    draw_shield(draw, cx, cy + size * 0.02, inner_size, fill="#1b2838")

    # Controller in the center
    draw_controller(draw, cx, cy - size * 0.03, size, "#66c0f4")

    # Keyhole below controller (vault motif)
    draw_keyhole(draw, cx, cy + size * 0.2, size, "#66c0f4")

    # Outer ring glow
    ring_w = max(2, int(size * 0.015))
    draw.ellipse(
        [margin, margin, size - margin, size - margin],
        fill=None,
        outline="#66c0f4",
        width=ring_w,
    )

    return img


def main():
    icon_dir = os.path.join(os.path.dirname(__file__), "..", "app", "src-tauri", "icons")
    os.makedirs(icon_dir, exist_ok=True)

    # Generate PNGs at required sizes
    sizes = {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 512,
    }

    for filename, size in sizes.items():
        img = generate_icon(size)
        path = os.path.join(icon_dir, filename)
        img.save(path, "PNG")
        print(f"Generated {filename} ({size}x{size})")

    # Generate ICO (Windows) - multiple sizes embedded
    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    ico_images = [generate_icon(s) for s in ico_sizes]
    ico_path = os.path.join(icon_dir, "icon.ico")
    ico_images[0].save(ico_path, format="ICO", sizes=[(s, s) for s in ico_sizes], append_images=ico_images[1:])
    print(f"Generated icon.ico ({len(ico_sizes)} sizes)")

    # Generate ICNS (macOS) - just save the 512px as PNG, Tauri handles conversion
    icns_path = os.path.join(icon_dir, "icon.icns")
    # For now, copy the 512px PNG as the icns source — Tauri's build will handle it
    # A proper .icns would need pyobjc or iconutil, but the PNG works for building
    big = generate_icon(512)
    big.save(icns_path, "PNG")  # Tauri accepts PNG for icns on build
    print("Generated icon.icns (512x512 PNG)")


if __name__ == "__main__":
    main()
