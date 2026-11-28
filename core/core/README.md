# whoisthat-core

`whoisthat-core` provides Xray, profiles, subscriptions, routing, TUN, and
kill-switch management through a length-prefixed JSON protocol. It runs
independently of the WhoisThat TUI and CLI.

## Install and integrate

Download the appropriate daemon archive and verify its checksum using the
[core releases](https://github.com/kvunoff/whoisthat/releases).
Run `./whoisthat-core` as your ordinary user for local proxy mode. To install
TUN support, use `sudo ./install-core --user "$USER" --tun` from the extracted
archive. Add `--service` for service setup and `--split` to configure split
tunneling with `whoisthat-run`.

See [Build & Run](https://docs.tuqise.com/developers/build) for the integration
sequence and [IPC & Wire Protocol](https://docs.tuqise.com/reference/protocol)
for commands and connection details.

## Build and test

Install the Go version declared in `go.mod`, then run:

```sh
go build -o whoisthat-core .
go test ./...
```

The standalone source repository is a generated mirror. Report issues and
submit changes to the [main WhoisThat repository](https://github.com/kvunoff/whoisthat).
