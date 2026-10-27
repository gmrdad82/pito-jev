#!/bin/sh
if [ -f "$1/stand_in.pid" ]; then
  kill "$(cat "$1/stand_in.pid")" 2> /dev/null || true
fi
