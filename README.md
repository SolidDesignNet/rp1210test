# rp1210test
rp1210 testing in Rust

```
joe@think:~/rp1210test$ target/debug/rp1210test 
Usage: rp1210test <COMMAND>

Commands:
  list    List available RP1210 adapters
  log     Log all traffic on specified adapter
  server  Respond to commands from other instances of rp1210test
  ping    Test latency
  tx      Test sending bandwidth
  rx      Test receiving bandwidth
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help information
joe@think:~/rp1210test$ target/debug/rp1210test log -h
Usage: rp1210test log [OPTIONS] --adapter <ADAPTER> --device <DEVICE>

Options:
  -a, --adapter <ADAPTER>
          RP1210 Adapter Identifier
  -d, --device <DEVICE>
          RP1210 Device ID
      --connection-string <CONNECTION_STRING>
          RP1210 Connection String [default: J1939:Baud=Auto]
      --address <ADDRESS>
          RP1210 Adapter Address (used for packets send and transport protocol) [default: F9]
  -h, --help
          Print help information
joe@think:~/rp1210test$ target/debug/rp1210test server -h
Usage: rp1210test server [OPTIONS] --adapter <ADAPTER> --device <DEVICE>

Options:
  -a, --adapter <ADAPTER>
          RP1210 Adapter Identifier
  -d, --device <DEVICE>
          RP1210 Device ID
      --connection-string <CONNECTION_STRING>
          RP1210 Connection String [default: J1939:Baud=Auto]
      --address <ADDRESS>
          RP1210 Adapter Address (used for packets send and transport protocol) [default: F9]
  -h, --help
          Print help information
joe@think:~/rp1210test$ target/debug/rp1210test ping -h
Usage: rp1210test ping [OPTIONS] --adapter <ADAPTER> --device <DEVICE> --count <COUNT>

Options:
  -a, --adapter <ADAPTER>
          RP1210 Adapter Identifier
  -d, --device <DEVICE>
          RP1210 Device ID
      --connection-string <CONNECTION_STRING>
          RP1210 Connection String [default: J1939:Baud=Auto]
      --address <ADDRESS>
          RP1210 Adapter Address (used for packets send and transport protocol) [default: F9]
      --dest <DEST>
          [default: 00]
  -c, --count <COUNT>
          
  -h, --help
          Print help information
joe@think:~/rp1210test$ 
```
