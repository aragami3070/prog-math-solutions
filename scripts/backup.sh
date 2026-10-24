#!/usr/bin/env bash
# Dumps the database, checks the dump is readable, writes its SHA-256, copies it to a
# second location and removes old dumps.
#
#   scripts/backup.sh [label]           e.g. scripts/backup.sh before-0008
#
# DATABASE_URL  database to dump                     (default: from .env)
# BACKUP_DIR     where dumps go                       (default ~/backups/rust-knowledge)
# BACKUP_MIRROR  second copy, ideally another disk    (default: none)
# BACKUP_KEEP    newest dumps to keep in BACKUP_DIR   (default 14; labelled ones are never removed)
set -euo pipefail

cd "$(dirname "$0")/.."
dir="${BACKUP_DIR:-$HOME/backups/rust-knowledge}"
keep="${BACKUP_KEEP:-14}"
label="${1:-}"
stamp="$(date +%Y%m%d-%H%M%S)"
name="rks-$stamp${label:+-$label}.dump"
if [[ -z "${DATABASE_URL:-}" && -f .env ]]; then
  DATABASE_URL="$(sed -n 's/^DATABASE_URL=//p' .env)"
fi
: "${DATABASE_URL:?DATABASE_URL is not set}"

mkdir -p "$dir"
trap 'rm -f "$dir/$name.partial"' ERR

pg_dump "$DATABASE_URL" --format=custom --no-owner > "$dir/$name.partial"
# A truncated or corrupt dump fails here instead of at restore time.
pg_restore --list < "$dir/$name.partial" > /dev/null
mv "$dir/$name.partial" "$dir/$name"
(cd "$dir" && sha256sum "$name" > "$name.sha256")

if [[ -n "${BACKUP_MIRROR:-}" ]]; then
  mkdir -p "$BACKUP_MIRROR"
  cp "$dir/$name" "$dir/$name.sha256" "$BACKUP_MIRROR/"
  (cd "$BACKUP_MIRROR" && sha256sum --check --quiet "$name.sha256")
fi

# Rotation covers only unlabelled dumps (rks-YYYYmmdd-HHMMSS.dump); names sort by time.
mapfile -t old < <(ls -1 "$dir" | grep -E '^rks-[0-9]{8}-[0-9]{6}\.dump$' | sort | head -n "-$keep")
for file in "${old[@]}"; do rm -f "$dir/$file" "$dir/$file.sha256"; done

echo "$dir/$name ($(du -h "$dir/$name" | cut -f1))"
