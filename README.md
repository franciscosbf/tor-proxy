# HTTP Tor Proxy

Pretty simple HTTP tor proxy which allows only [CONNECT requests](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Methods/CONNECT) and pipes communications through Tor network using [Arti](https://tpo.pages.torproject.net/core/arti/). It tries to isolate connections into separate Tor circuits based on second level domain, caching circuits per each one. If a CONNECT request provides an IP, the proxy will try to isolate the connection into a different circuit as well, caching its corresponding circuit as well (this may eventually clash with the corresponding domain, but it is what it is).

### Security Concerns

As an aside, this obviously isn't as secure as Tor browser itself or Tor SOCKS protocol extension. For example, by analysing [2.2 Privacy Requirements](https://gitlab.torproject.org/tpo/applications/wiki/-/wikis/Design-Documents/Tor-Browser-Design-Doc#22-privacy-requirements) section of Tor Browser Design Doc, my implementation doesn't follow the same demands regarding "URL bar origin", as it simply can't figure out were the f*** a given request is actually coming from. Furthermore, Tor SOCKS extension allows to request different circuit per communication stream, known as [Stream Isolation](https://spec.torproject.org/socks-extensions), which, for the same reason, is not possible in this case.
