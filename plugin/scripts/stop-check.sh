#!/bin/sh
# Consortium Stop hook.
#
# The hard problem with two agents talking is that an agent stops existing the
# moment its turn ends, so a reply that lands a second later reaches nobody.
# This hook runs as the turn is about to end: if anything is waiting in the room
# it refuses the stop and hands the messages back, so the session keeps going.
#
# Terminating by construction: `consortium read` consumes what it returns, so a
# turn can only be extended by genuinely new traffic, never by the same message
# twice.

command -v consortium >/dev/null 2>&1 || exit 0        # not installed: never interfere

ME="${CONSORTIUM_NAME:-Claude}"
WS="$(consortium who 2>/dev/null)"
LOG="$WS/.consortium/messages.jsonl"

# Only hold the turn open while the room is actually live. Working alone should
# not cost a delay at the end of every turn.
HOLD=0
if [ -f "$LOG" ]; then
  NOW=$(date +%s)
  MTIME=$(stat -f %m "$LOG" 2>/dev/null || stat -c %Y "$LOG" 2>/dev/null || echo 0)
  AGE=$((NOW - MTIME))
  IDLE_WINDOW="${CONSORTIUM_IDLE_WINDOW:-600}"
  [ "$AGE" -lt "$IDLE_WINDOW" ] && HOLD="${CONSORTIUM_STOP_WAIT:-45}"
fi

# Drain the backlog FIRST. `wait` only reports messages posted after it starts,
# so waiting without reading would sail straight past a message that arrived
# mid-turn — precisely the case this hook exists to catch.
OUT="$(consortium read "$ME" 2>/dev/null)"

case "$OUT" in
  ""|"(nothing new)"*)
    # Nothing queued. Hold briefly for a reply still in flight, if the room is live.
    if [ "$HOLD" -gt 0 ]; then
      OUT="$(consortium wait "$ME" --secs "$HOLD" 2>/dev/null)"
    else
      OUT=""
    fi
    ;;
esac

case "$OUT" in
  ""|"(nothing new)"*|"(nothing after"*)
    exit 0 ;;                                          # room is quiet: let the turn end
esac

# Something is waiting. Refuse the stop and hand it to the model.
python3 - "$ME" "$OUT" <<'PY'
import json, sys
me, out = sys.argv[1], sys.argv[2]
print(json.dumps({
    "decision": "block",
    "reason": (
        "New messages arrived in the Consortium room while you were finishing. "
        "You are '" + me + "'. Read them and respond in character; reply with "
        "`consortium post " + me + " \"...\"`. If nothing is needed from you, "
        "say so briefly and stop.\n\n" + out
    )
}))
PY
exit 0
