#!/usr/bin/env bash
# Generate the README screenshots from synthetic data.
#
# The app runs inside a sandbox: a temporary HOME (it never reads or writes
# ~/.config/gdrive), its own D-Bus session (it cannot disturb a running
# instance), its own X session (Xvfb) and an rclone `demo:` remote pointing at
# an invented tree. The images therefore cannot leak the operator's paths,
# accounts or files.
#
# Requires: Xvfb, ffmpeg with x11grab, rclone, python3 with Pillow, and a built
# daemon (native/gdrive-core/target/release/gdrive-daemon).
#
# Usage: tools/demo-screenshot.sh [output dir]      (default: docs/images)

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO/docs/images}"
DAEMON="$REPO/native/gdrive-core/target/release/gdrive-daemon"
APP="$REPO/gdrive-app.py"

for req in Xvfb ffmpeg rclone; do
    command -v "$req" >/dev/null || { echo "missing dependency: $req" >&2; exit 1; }
done
[ -x "$DAEMON" ] || { echo "daemon not built: run cargo build --release in native/gdrive-core" >&2; exit 1; }
[ -f "$APP" ] || { echo "cannot find $APP" >&2; exit 1; }
python3 -c "import PIL" 2>/dev/null || { echo "missing python3 Pillow" >&2; exit 1; }

BOX="$(mktemp -d)"
trap 'rm -rf "$BOX"' EXIT
mkdir -p "$BOX/home/.config/gdrive" "$BOX/tree/Documentos/Proyecto Alpha" \
         "$BOX/tree/Documentos/Informes" "$BOX/tree/Fotos/2026" "$BOX/tree/Audios"
mkdir -p "$OUT_DIR"

# Demo tree: generic names and meaningless content.
for f in "$BOX/tree/Documentos/Proyecto Alpha/notas.md" \
         "$BOX/tree/Documentos/Proyecto Alpha/plan.md" \
         "$BOX/tree/Documentos/Informes/informe.md" \
         "$BOX/tree/Fotos/2026/foto-ejemplo.jpg" \
         "$BOX/tree/Audios/grabacion.mp3"; do
    head -c 4096 /dev/zero > "$f"
done
mkdir -p "$BOX/home/GoogleDrive"
ln -s "$BOX/tree/Documentos" "$BOX/home/GoogleDrive/Documentos"
ln -s "$BOX/tree/Fotos" "$BOX/home/GoogleDrive/Fotos"
ln -s "$BOX/tree/Audios" "$BOX/home/GoogleDrive/Audios"

# Icons: if setup.sh already ran, reuse the installed set read-only so the
# screenshot matches the real app. Without them GTK falls back to defaults.
if [ -d "$HOME/.local/share/icons/hicolor" ]; then
    mkdir -p "$BOX/home/.local/share"
    ln -s "$HOME/.local/share/icons" "$BOX/home/.local/share/icons"
fi

# Synthetic settings: starts from the project defaults and pins an example
# path. start_hidden is False so the window shows up without being asked for.
python3 - "$REPO" "$BOX" <<'PY'
import json, pathlib, sys
sys.path.insert(0, sys.argv[1])
from gdrive.config import DEFAULTS
c = dict(DEFAULTS)
c.update({
    "sync_dir": "/home/usuario/GoogleDrive",
    "language": "es",
    "sync_interval": "15min",
    "bandwidth_limit": "2M",
    "start_hidden": False,
    "last_sync": "",
    "last_sync_ok": False,
})
d = pathlib.Path(sys.argv[2]) / "home/.config/gdrive"
d.mkdir(parents=True, exist_ok=True)
(d / "settings.json").write_text(json.dumps(c, indent=2))
PY

HOME="$BOX/home" RCLONE_CONFIG="$BOX/home/rclone.conf" \
    rclone config create demo alias remote="$BOX/tree/" >/dev/null 2>&1

cat > "$BOX/inner.sh" <<'INNER'
#!/usr/bin/env bash
# Dentro de la caja: DISPLAY, HOME y RCLONE_CONFIG ya estan puestos.
set -u
export GDK_BACKEND=x11

recortar() {
    python3 - "$1" <<'PY'
import sys
from PIL import Image, ImageChops
p = sys.argv[1]
im = Image.open(p).convert("RGB")
diff = ImageChops.difference(im, Image.new("RGB", im.size, (0, 0, 0)))
box = diff.getbbox()
if box:
    im.crop(box).save(p)
PY
}

contenido() {
    python3 - "$1" <<'PY'
import sys
from PIL import Image
try:
    im = Image.open(sys.argv[1]).convert("RGB")
    colors = im.getcolors(400000) or []
    black = sum(n for n, px in colors if px == (0, 0, 0))
    print(100 - 100 * black // (im.size[0] * im.size[1]))
except Exception:
    print(0)
PY
}

capturar() {  # $1 = destino .png; reintenta hasta que la ventana este dibujada
    local i px
    for i in 1 2 3 4 5 6 7 8; do
        # -draw_mouse 0 va ANTES de -i: es una opcion de la entrada x11grab.
        ffmpeg -loglevel error -f x11grab -video_size 1180x780 -draw_mouse 0 \
            -i "$DISPLAY" -frames:v 1 "$1" -y 2>/dev/null
        px=$(contenido "$1")
        [ "${px:-0}" -gt 30 ] && { recortar "$1"; return 0; }
        sleep 3
    done
    echo "  ! $1 still blank after 8 attempts" >&2
    return 1
}

cd "$1" || exit 1
"$2" >/dev/null 2>&1 &
DAE=$!
sleep 2
python3 "$3" >/dev/null 2>&1 &
APP=$!
sleep 8
capturar "$4/screenshot-status.png"
gapplication activate com.gdrive.ClientUI 2>/dev/null || true
gapplication action com.gdrive.ClientUI go-settings 2>/dev/null || true
sleep 3
capturar "$4/screenshot-settings.png"
kill "$APP" "$DAE" 2>/dev/null
wait 2>/dev/null
INNER

mkdir -p "$BOX/dest"
HOME="$BOX/home" GDRIVE_REMOTE="demo:" RCLONE_CONFIG="$BOX/home/rclone.conf" \
    REPO="$REPO" DAEMON="$DAEMON" APP="$APP" DEST="$BOX/dest" \
    dbus-run-session -- xvfb-run -a -s "-screen 0 1180x780x24" \
    bash "$BOX/inner.sh" "$REPO" "$DAEMON" "$APP" "$BOX/dest"

cp "$BOX/dest"/*.png "$OUT_DIR"/ 2>/dev/null || {
    echo "no screenshots were generated" >&2; exit 1; }
ls -la "$OUT_DIR"/screenshot-*.png | awk '{print $5, $9}'
