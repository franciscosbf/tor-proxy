# HTTP Tor Proxy

Pretty simple HTTP tor proxy which allows only [CONNECT requests](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Methods/CONNECT) and pipes communications through Tor network using [Arti](https://tpo.pages.torproject.net/core/arti/). It tries to isolate connections into separate Tor circuits based on second level domain, caching circuits per each one. If a CONNECT request provides an IP, the proxy will try to isolate the connection into a different circuit as well, caching its corresponding circuit as well (this may eventually clash with the corresponding domain, but it is what it is).

### Compiling

Quick-and-dirty solution for safe compiling programs shipped with Arti (see more [here](https://tpo.pages.torproject.net/core/arti/guides/safer-build-options)):

```sh
RUSTFLAGS="--remap-path-prefix $HOME/.cargo=.cargo --remap-path-prefix $(pwd)=. --remap-path-prefix $HOME/.rustup=.rustup" \
   cargo build --release
```

### Usage

Use the `--help` flag to see which options are available.

```
$ ./tor-proxy --help

Tunnels HTTP communications through Tor network

Usage: tor-proxy [OPTIONS]

Options:
  -p, --port <PORT>                  Port were proxy will listening on [default: 8080]
  -r, --replenish <REPLENISH>        GCRA limiter replenish interval in seconds (time it takes to replenish a single cell during exaustion) [default: 4]
      --max-burst <MAX_BURST>        GCRA limiter max burst size until triggered [default: 100]
  -c, --circuits <CIRCUITS>          Min number of Tor circuits established after client boostrap [default: 12]
      --max-entries <MAX_ENTRIES>    Max capacity of cached Tor clients [default: 100]
  -t, --ttl <TTL>                    Time to live in seconds per cached Tor client [default: 3600]
  -i, --incoming-buf <INCOMING_BUF>  Connection buffer between user and proxy [default: 512B]
  -o, --outgoing-buf <OUTGOING_BUF>  Connection buffer between proxy and Tor network [default: 512B]
  -d, --debug                        Increase tracing verbosity
  -h, --help                         Print help
  -V, --version                      Print version
```

`--replenish` and `--max-burst` allow to configure the [GCRA](https://en.wikipedia.org/wiki/Generic_cell_rate_algorithm) limiter for incoming connections. `--ttl` controls how long the circuit is cached for a given second level domain. `--incoming-buf` and `--outgoing-buf` are applied on each connection made and its associated tunnel to Tor network.

### Security Concerns

As an aside, this obviously isn't as secure as Tor browser itself or Tor SOCKS protocol extension. For example, by analysing [2.2 Privacy Requirements](https://gitlab.torproject.org/tpo/applications/wiki/-/wikis/Design-Documents/Tor-Browser-Design-Doc#22-privacy-requirements) section of Tor Browser Design Doc, my implementation doesn't follow the same demands regarding "URL bar origin", as it simply can't figure out were the f*** a given request is actually coming from. Furthermore, Tor SOCKS extension allows to request different circuit per communication stream, known as [Stream Isolation](https://spec.torproject.org/socks-extensions), which, for the same reason, is not possible in this case.
