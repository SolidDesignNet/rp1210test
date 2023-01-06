#!/bin/bash
cargo build --release
RP1210TEST=./target/release/rp1210test 

ADAPTERS=("NULN2R32 1" "PEAKRP32 1 --connection-string J1939:Baud=500")

COUNT=1000

for s in "${ADAPTERS[@]}"
do
  $RP1210TEST server $s >& /dev/null &

  for a in "${ADAPTERS[@]}"
  do
    if [[ "$a" != "$s" ]]
    then
      echo "$s   ==>>   $a"
      $RP1210TEST composite $a --address F8 --dest F9 --count $COUNT $@
      echo
    fi
  done

  kill %1
  #  $RP1210TEST exit $s
done

kill 0