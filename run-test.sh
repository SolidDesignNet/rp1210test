#!/bin/bash
cargo build --release
RP1210TEST=./target/release/rp1210test 

SERVERS=("NULN2R32 1" "PEAKRP32 1 --connection-string J1939:Baud=500" "VRP32 1 --connection-string J1939:Baud=500")
CLIENTS=()
ALL=("${SERVERS[@]}" "${CLIENTS[@]}")

COUNT=1000

for s in "${SERVERS[@]}"
do
  $RP1210TEST server $s --address F9 > /dev/null &
  sleep 5
  
  for a in "${ALL[@]}"
  do
    if [[ "$a" != "$s" ]]
    then
      echo "$a   ==>>   $s"
      $RP1210TEST composite $a --address F8 --dest F9 --count $COUNT $@
      echo
      sleep 1
    fi
  done

  kill -9 %1
  sleep 5
done

kill 0