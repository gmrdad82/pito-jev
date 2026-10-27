#!/bin/sh
set -eu
mkdir -p "$2"
nohup "$1/stand_in" 8472 demo-key > "$2/stand_in.log" 2>&1 &
echo $! > "$2/stand_in.pid"
sleep 0.3
