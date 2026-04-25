#!/usr/bin/env bash
# ─── download-samples.sh ──────────────────────────────────────────────────────
# Download CC-licensed sample packs for Impulse Instruct.
#
# Usage:
#   ./scripts/download-samples.sh                  # show this help
#   ./scripts/download-samples.sh salamander       # Salamander Grand Piano (~730 MB, SFZ)
#   ./scripts/download-samples.sh sso              # Sonatina Symphonic Orchestra (~1.3 GB, SFZ)
#   ./scripts/download-samples.sh vsco2            # VSCO 2 Community Edition (~2.3 GB, SFZ, CC0)
#   ./scripts/download-samples.sh instruments-all  # all three SFZ instrument packs (~4.4 GB)
#   ./scripts/download-samples.sh amen             # print curated source URLs for amen breaks
#   ./scripts/download-samples.sh textures         # print curated source URLs for granular textures
#   ./scripts/download-samples.sh wavetables       # print curated source URLs for Serum-style wavetables
#   ./scripts/download-samples.sh impulses         # print curated source URLs for IRs (convolution reverb)
#
# AUTOMATED PACKS (instruments) fetch the official GitHub mirrors into
# samples/instruments/<pack>/.  Uses `git clone --depth 1` when git is
# available; otherwise falls back to the GitHub zipball
# (/archive/refs/heads/master.zip) via curl/wget + unzip — so end-user
# binaries without git installed still work.  The SAMPLER+ module's
# LOAD button can navigate into the subfolder to pick a .sfz.
#
# REFERENCE-ONLY PACKS (amen / textures / wavetables / impulses) print
# the curated source URLs from samples/README.md — these libraries don't
# ship clean direct-download archives we can pull non-interactively, so
# the script just points you at the right place to grab them.
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
printf "\n  \033[38;2;110;110;110m▁▂▄▅▇██▇▅▄▂▁▁▂▄▅▇█\033[38;2;160;160;160m I M P U L S E • I N S T R U C T \033[38;2;110;110;110m█▇▅▄▂▁▁▂▄▅▇██▇▅▄▂▁\033[0m\n\n"
cd "$(dirname "$0")/.."

INSTR_DIR="samples/instruments"
mkdir -p "$INSTR_DIR" samples/amen samples/textures samples/wavetables samples/impulses

# Absolute paths so users know exactly where files belong on disk —
# the script always cd's to the repo root, so $(pwd) is canonical.
REPO_ROOT="$(pwd)"
ABS_INSTR="${REPO_ROOT}/samples/instruments"
ABS_AMEN="${REPO_ROOT}/samples/amen"
ABS_TEXTURES="${REPO_ROOT}/samples/textures"
ABS_WAVETABLES="${REPO_ROOT}/samples/wavetables"
ABS_IMPULSES="${REPO_ROOT}/samples/impulses"

PACK="${1:-}"

show_usage() {
  cat <<'EOF'
Usage: ./scripts/download-samples.sh <pack>

Automated (git clone, drops into samples/instruments/<pack>/):
  salamander         Salamander Grand Piano V3       (~730 MB)
  sso                Sonatina Symphonic Orchestra    (~1.3 GB)
  vsco2              VSCO 2 Community Edition (CC0)  (~2.3 GB)
  instruments-all    all three of the above          (~4.4 GB total)

Reference-only (prints curated source URLs):
  amen               Amen breaks → samples/amen/
  textures           Granular textures → samples/textures/
  wavetables         Serum-style wavetables → samples/wavetables/
  impulses           IRs for convolution reverb → samples/impulses/

See samples/README.md for the long-form pack notes.
EOF
}

# ── Helpers ───────────────────────────────────────────────────────────────────
confirm_size() {
  local name="$1" size="$2"
  echo ""
  echo "  About to download ${name} (${size}) into ${INSTR_DIR}/."
  read -r -p "  Continue? [Y/n] " reply
  reply="${reply:-y}"
  [[ "$reply" =~ ^[Yy]$ ]]
}

# Fall back to the GitHub zipball when git isn't installed.  Public
# repos serve a no-auth zip at /archive/refs/heads/<branch>.zip; we
# extract with `unzip` (preinstalled on macOS, usually on Linux) and
# rename the extracted `<repo>-<branch>/` folder to the canonical name.
fetch_zipball() {
  local repo="$1" branch="$2" dest="$3"
  local repo_name="${repo##*/}"
  local tmp_zip="${dest}.zip"
  local url="https://github.com/${repo}/archive/refs/heads/${branch}.zip"

  if ! command -v unzip &>/dev/null; then
    echo "ERROR: 'unzip' not found.  Install one of:"
    echo "  Debian/Ubuntu:  sudo apt install unzip"
    echo "  Fedora/RHEL:    sudo dnf install unzip"
    echo "  macOS:          (preinstalled — should never see this)"
    echo "Or install git so the script can clone instead:"
    echo "  Debian/Ubuntu:  sudo apt install git"
    return 1
  fi

  echo "Downloading ${url}"
  if command -v curl &>/dev/null; then
    curl -fL --progress-bar -o "$tmp_zip" "$url" || { rm -f "$tmp_zip"; return 1; }
  elif command -v wget &>/dev/null; then
    wget --progress=bar:force -O "$tmp_zip" "$url" || { rm -f "$tmp_zip"; return 1; }
  else
    echo "ERROR: need 'git', 'curl', or 'wget' — none found."
    return 1
  fi

  echo "Extracting → ${dest}"
  unzip -q "$tmp_zip" -d "${INSTR_DIR}"
  rm -f "$tmp_zip"
  if [[ -d "${INSTR_DIR}/${repo_name}-${branch}" ]]; then
    mv "${INSTR_DIR}/${repo_name}-${branch}" "$dest"
  fi
}

# Try git first (cheaper + matches CI / dev environments), fall back
# to the GitHub zipball for end users without git installed.
clone_pack() {
  local repo="$1" dest_name="$2" size="$3"
  local dest="${INSTR_DIR}/${dest_name}"

  if [[ -d "$dest" ]]; then
    echo "✓ ${dest_name} already present at ${dest}"
    echo "  Delete the directory to re-download."
    return 0
  fi

  if ! confirm_size "${dest_name}" "${size}"; then
    echo "  Skipped."
    return 0
  fi

  echo ""
  if command -v git &>/dev/null; then
    echo "Cloning https://github.com/${repo}.git → ${dest}"
    git clone --depth 1 "https://github.com/${repo}.git" "$dest"
  else
    echo "git not found — falling back to GitHub zipball download."
    fetch_zipball "$repo" "master" "$dest" || return 1
  fi

  local actual
  actual=$(du -sh "$dest" 2>/dev/null | cut -f1 || echo "?")
  echo ""
  echo "✓ ${dest_name} ready (${actual} on disk)"
  echo "  Location: ${REPO_ROOT}/${dest}/"
  echo "  Load via the SAMPLER+ card's LOAD button — the file dialog can"
  echo "  navigate into the pack folder to pick a .sfz."
}

# ── Reference-only printers (pulled from samples/README.md) ──────────────────
print_amen() {
  cat <<EOF
─── Amen breaks ────────────────────────────────────────────────────────────────

  PLACE FILES HERE:  ${ABS_AMEN}/

The AMEN sampler module reads .wav files from that folder.  Curated sources:

  https://archive.org/details/amen-breaks
  https://archive.org/details/amen-breaks-compilation

Workflow:
  1. Download a .zip from one of the archive.org pages above.
  2. Extract the .wav files into  ${ABS_AMEN}/
  3. The module's file picker lists them automatically on next launch.

EOF
}

print_textures() {
  cat <<EOF
─── Granular textures ──────────────────────────────────────────────────────────

  PLACE FILES HERE:  ${ABS_TEXTURES}/

The GRAN granular texture module reads .wav files from that folder.
Longer, slowly-evolving material grains best (pads, drones, field recordings).

Curated sources:

  https://archive.org/details/opensource_audio   mixed-bag (check per-item license)
  https://archive.org/details/audio_ambient      ambient and drone uploads
  https://freesound.org                          search: drone / pad / texture / field

Workflow:
  1. Pick a CC0 / CC-BY upload from one of the pages above.
  2. Drop the .wav into  ${ABS_TEXTURES}/
  3. The granular voice's picker lists them automatically.

EOF
}

print_wavetables() {
  cat <<EOF
─── Wavetables ─────────────────────────────────────────────────────────────────

  PLACE FILES HERE:  ${ABS_WAVETABLES}/

The WAVETABLE voice reads Serum-style frame-stack .wav files (2048-sample
frames concatenated into one buffer) from that folder.

Curated sources:

  https://wavetables.com                                    large CC0 collection
  https://waveedit.online                                   browseable single-cycles
  https://www.adventurekid.se/akrt/                         AKWF — Adventure Kid free

Workflow:
  1. Download a Serum-format .wav (any frame count).
  2. Drop it into  ${ABS_WAVETABLES}/
  3. Load via the WAVETABLE card's LOAD button or POST /api/wavetable.

EOF
}

print_impulses() {
  cat <<EOF
─── Impulse responses ──────────────────────────────────────────────────────────

  PLACE FILES HERE:  ${ABS_IMPULSES}/

The CONV REV convolution-reverb module reads .wav IRs from that folder.
Short IRs (0.5 – 2 s) work best for musical reverb.

Curated sources:

  https://archive.org/details/ir-library    halls, plates, outdoor spaces
  https://openairlib.net                    academic IR archive (Univ. of York)
  https://www.voxengo.com/impulses/         small Voxengo free pack

Workflow:
  1. Download an IR .wav (any sample rate; the loader resamples).
  2. Drop it into  ${ABS_IMPULSES}/
  3. Load via the ConvReverb card's LOAD IR button or POST /api/conv_reverb.

EOF
}

# ── Dispatch ──────────────────────────────────────────────────────────────────
case "$PACK" in
  salamander)
    clone_pack "sfzinstruments/SalamanderGrandPiano" "SalamanderGrandPiano" "~730 MB"
    ;;
  sso)
    clone_pack "peastman/sso" "sso" "~1.3 GB"
    ;;
  vsco2)
    clone_pack "sgossner/VSCO-2-CE" "VSCO-2-CE" "~2.3 GB"
    ;;
  instruments-all)
    clone_pack "sfzinstruments/SalamanderGrandPiano" "SalamanderGrandPiano" "~730 MB"
    clone_pack "peastman/sso" "sso" "~1.3 GB"
    clone_pack "sgossner/VSCO-2-CE" "VSCO-2-CE" "~2.3 GB"
    ;;
  amen)       print_amen ;;
  textures)   print_textures ;;
  wavetables) print_wavetables ;;
  impulses)   print_impulses ;;
  ""|help|-h|--help)
    show_usage
    exit 0
    ;;
  *)
    echo "Unknown pack: '$PACK'"
    echo ""
    show_usage
    exit 1
    ;;
esac

echo ""
echo "─── License notice ─────────────────────────────────────────────────────────"
case "$PACK" in
  salamander)
    echo "Salamander Grand Piano V3 by Alexander Holm — CC-BY 3.0."
    echo "See: https://github.com/sfzinstruments/SalamanderGrandPiano"
    ;;
  sso)
    echo "Sonatina Symphonic Orchestra by Mattias Westlund (mirror by Peter Eastman)."
    echo "Released under SSO's free-use terms; see LICENSE in the cloned repo."
    echo "Repo: https://github.com/peastman/sso"
    ;;
  vsco2)
    echo "VSCO 2 Community Edition by Versilian Studios — CC0 1.0 (public domain)."
    echo "See: https://github.com/sgossner/VSCO-2-CE"
    ;;
  instruments-all)
    echo "Salamander Grand Piano: CC-BY 3.0  (Alexander Holm)"
    echo "Sonatina Symphonic Orchestra: free-use terms (Mattias Westlund / Peter Eastman)"
    echo "VSCO 2 CE: CC0 1.0 (Versilian Studios)"
    ;;
  amen|textures|wavetables|impulses)
    echo "Per-pack licenses vary — check each download's terms before redistribution."
    ;;
esac
echo "─────────────────────────────────────────────────────────────────────────────"
