"""Resize the Recraft icon, strip white background, generate all Tauri icon sizes."""

from PIL import Image
import os


def strip_white_background(img):
    """Replace white/near-white pixels with transparency."""
    img = img.convert("RGBA")
    data = img.getdata()
    new_data = []
    for r, g, b, a in data:
        # If pixel is white or near-white, make transparent
        if r > 240 and g > 240 and b > 240:
            new_data.append((0, 0, 0, 0))
        else:
            new_data.append((r, g, b, a))
    img.putdata(new_data)
    return img


def main():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    # Try to find the source image
    source_candidates = [
        os.path.join(script_dir, "..", "flat-minimal-app-icon-for-a-desktop-application-ca (1).png"),
        os.path.join(script_dir, "..", "icon_source.png"),
    ]
    source = None
    for s in source_candidates:
        if os.path.exists(s):
            source = s
            break

    if source is None:
        # Use the existing icon.png as source if no original found
        source = os.path.join(script_dir, "..", "app", "src-tauri", "icons", "icon.png")

    icon_dir = os.path.join(script_dir, "..", "app", "src-tauri", "icons")
    os.makedirs(icon_dir, exist_ok=True)

    img = Image.open(source).convert("RGBA")
    print(f"Source: {img.size[0]}x{img.size[1]}")

    # Strip white background
    img = strip_white_background(img)
    print("Stripped white background")

    # Generate PNGs at required sizes
    sizes = {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 512,
    }
    for filename, size in sizes.items():
        resized = img.resize((size, size), Image.LANCZOS)
        path = os.path.join(icon_dir, filename)
        resized.save(path, "PNG")
        print(f"Generated {filename} ({size}x{size})")

    # Generate ICO (Windows) — multiple sizes
    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    ico_images = [img.resize((s, s), Image.LANCZOS) for s in ico_sizes]
    ico_path = os.path.join(icon_dir, "icon.ico")
    ico_images[0].save(
        ico_path, format="ICO",
        sizes=[(s, s) for s in ico_sizes],
        append_images=ico_images[1:],
    )
    print(f"Generated icon.ico ({len(ico_sizes)} sizes)")

    # ICNS — save 512px PNG (Tauri handles conversion)
    icns_path = os.path.join(icon_dir, "icon.icns")
    img.resize((512, 512), Image.LANCZOS).save(icns_path, "PNG")
    print("Generated icon.icns (512x512)")


if __name__ == "__main__":
    main()
