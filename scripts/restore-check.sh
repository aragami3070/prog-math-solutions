#!/usr/bin/env bash
# Restore drill: dump the live database, restore it into a scratch database, compare, rebuild
# the search index from the restored copy and check that it answers the evaluation queries
# exactly like the live index. Reads the live database; writes only to a scratch database
# (dropped at the end), a scratch index directory and two scratch run files.
#
#   scripts/restore-check.sh            prints a report with the measured times
#
# Needs: docker-compose with the compose `postgres` service up (pg_dump/pg_restore run inside it,
# so their version always matches the server), psql on PATH, DATABASE_URL (or .env), the `eval` binary built
# (cargo build --bin eval) and a live index in SEARCH_INDEX_DIR (default data/index).
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -z "${DATABASE_URL:-}" && -f .env ]]; then DATABASE_URL="$(sed -n 's/^DATABASE_URL=//p' .env)"; fi
: "${DATABASE_URL:?DATABASE_URL is not set}"
export SQLX_OFFLINE=true
live="$DATABASE_URL"
scratch_db="rks_restore_check"
admin="${live%/*}/postgres"
restored="${live%/*}/$scratch_db"
work="$(mktemp -d)"
trap 'psql "$admin" -qc "DROP DATABASE IF EXISTS $scratch_db" >/dev/null 2>&1 || true; rm -rf "$work" eval/runs/restore-live.* eval/runs/restore-copy.*' EXIT
EVAL=target/debug/eval
[[ -x "$EVAL" ]] || EVAL=target/release/eval

t() { date +%s.%N; }
secs() { awk -v a="$1" -v b="$2" 'BEGIN{printf "%.1f", b-a}'; }

s=$(t); docker-compose exec -T postgres pg_dump -U rks -d rks --format=custom --no-owner > "$work/db.dump"; e=$(t)
dump_s=$(secs "$s" "$e"); size=$(du -h "$work/db.dump" | cut -f1)
docker-compose exec -T postgres pg_restore --list < "$work/db.dump" > /dev/null

psql "$admin" -qc "DROP DATABASE IF EXISTS $scratch_db" -c "CREATE DATABASE $scratch_db"
s=$(t); docker-compose exec -T postgres pg_restore -U rks --no-owner -d "$scratch_db" < "$work/db.dump"; e=$(t)
restore_s=$(secs "$s" "$e")

# Same rows, same content: row counts per table and a fingerprint of every document.
fingerprint() {
  psql "$1" -Atc "SELECT 'documents', count(*), md5(string_agg(source||':'||native_id||':'||content_hash||':'||completeness, ',' ORDER BY id)) FROM documents
   UNION ALL SELECT 'posts', count(*), md5(string_agg(document_id||':'||post_number||':'||md5(text), ',' ORDER BY id)) FROM posts
   UNION ALL SELECT 'links', count(*), md5(string_agg(document_id||':'||target_url, ',' ORDER BY id)) FROM links
   UNION ALL SELECT 'edges', count(*), md5(string_agg(source_id||'>'||target_id||':'||occurrences, ',' ORDER BY id)) FROM edges
   UNION ALL SELECT 'crawl_queue', count(*), md5(string_agg(status, ',' ORDER BY id)) FROM crawl_queue"
}
fingerprint "$live" > "$work/live.fp"; fingerprint "$restored" > "$work/copy.fp"
if diff -q "$work/live.fp" "$work/copy.fp" >/dev/null; then data="identical"; else data="DIFFERENT"; diff "$work/live.fp" "$work/copy.fp" || true; fi

# Derived data is rebuilt, not restored: index from the restored database.
s=$(t); DATABASE_URL="$restored" SEARCH_INDEX_DIR="$work/index" "$EVAL" rebuild > "$work/rebuild.out"; e=$(t)
rebuild_s=$(secs "$s" "$e")

DATABASE_URL="$live"     "$EVAL" run --variant restore-live > /dev/null
DATABASE_URL="$restored" SEARCH_INDEX_DIR="$work/index" "$EVAL" run --variant restore-copy > /dev/null
if diff -q <(cut -f1-3 eval/runs/restore-live.tsv) <(cut -f1-3 eval/runs/restore-copy.tsv) >/dev/null; then answers="identical"; else answers="DIFFERENT"; fi
queries=$(cut -f1 eval/runs/restore-live.tsv | sort -u | wc -l)

cat <<REPORT
Restore drill ($(date -u +%Y-%m-%dT%H:%M:%SZ))
- dump: ${dump_s} s, ${size}; pg_restore into a scratch database: ${restore_s} s
- tables compared (documents, posts, links, edges, crawl_queue; count + content fingerprint): ${data}
- index rebuilt from the restored database: ${rebuild_s} s
- top results of ${queries} evaluation queries, live index vs rebuilt index (document and rank): ${answers}
REPORT
[[ "$data" == identical && "$answers" == identical ]]
