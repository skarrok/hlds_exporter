# hlds_exporter

Export metrics from HLDS server in prometheus format using [Server Queries](https://developer.valvesoftware.com/wiki/Server_queries).

Notice: only tested with Counter-Strike 1.6

## Quickstart

```bash
docker run --init -it -p 9000:9000 -e SERVER_ADDR=91.211.115.172:27015 -e=METRICS_ADDR=0:9000 skarrok/hlds_exporter:v0.1.0
```
Now you can query exporter with curl on 127.0.0.1:9000 or connect it to prometheus.

## Configuration

Use can configure exporter with command line arguments, environment variables and `.env` file.

```
HLDS metrics exporter in prometheus format

Usage: hlds_exporter [OPTIONS]

Options:
      --log-level <LOG_LEVEL>
          Verbosity of logging

          [env: LOG_LEVEL=]
          [default: debug]
          [possible values: off, trace, debug, info, warn, error]

      --log-format <LOG_FORMAT>
          Format of logs

          [env: LOG_FORMAT=]
          [default: console]

          Possible values:
          - console: Pretty logs for debugging
          - json:    JSON logs

      --metrics-addr <METRICS_ADDR>
          Address for exporting metrics

          [env: METRICS_ADDR=127.0.0.1:9000]
          [default: 127.0.0.1:9000]

      --server-addr <SERVER_ADDR>...
          HLDS Server Addresses

          [env: SERVER_ADDR=skarrok.com:27015]
          [default: 127.0.0.1:27015]

      --listen-addr <LISTEN_ADDR>
          UDP Bind Address

          [env: LISTEN_ADDR=]
          [default: 0.0.0.0:0]

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

## Building

Just clone this repository and run:

```bash
cargo build --release
```

## Example metrics output
```
# HELP hlds_info server info.
# TYPE hlds_info gauge
hlds_info{name="Kreedz Jump Server",addr="91.211.115.172:27015",game="Counter-Strike",version="1.1.2.7/Stdio"} 1
# HELP hlds_players current number of players.
# TYPE hlds_players gauge
hlds_players{addr="91.211.115.172:27015"} 1
# HELP hlds_bots current number of bots.
# TYPE hlds_bots gauge
hlds_bots{addr="91.211.115.172:27015"} 1
# HELP hlds_up server is up.
# TYPE hlds_up gauge
hlds_up{addr="91.211.115.172:27015"} 1
# EOF
```
