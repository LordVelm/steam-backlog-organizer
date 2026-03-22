"""Resize the Recraft icon to all required Tauri icon sizes."""

from PIL import Image
import os

def main():
    script_dir = os.path.dirname(__file__)
    source = os.path.join(script_dir, "..", "flat-minimal-app-icon-for-a-desktop-application-ca (1).png")
    icon_dir = os.path.join(script_dir, "..", "app", "src-tauri", "icons")
    os.makedirs(icon_dir, exist_ok=True)

    img = Image.open(source).convert("RGBA")
    print(f"Source: {img.size[0]}x{img.size[1]}")

    # Generate PNGs
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
