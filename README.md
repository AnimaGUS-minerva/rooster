Hermes Rooster --- RFC8995 Join Proxy
-------------------------------------

This is an implementation of the Join Proxy described in RFC8995 section 4.

It is presently stateful, supporting only HTTPS connections.
Future versions will support CoAPS connections as described in

1. draft-ietf-anima-constrained-voucher
2. draft-ietf-anima-constrained-join-proxy

Rooster listens for GRASP M_FLOOD messages announing the location of the AN\_Register.  This is usually done on an ACP interface, but it may use any interface if desired.

Rooster then sends GRASP M_FLOOD message announcing itself using the AN\_join\_proxy
M_FLOOD on some set of interfaces, usually all interfaces.

Rooster listens on the port that it has bound for connections, and when it receives them it connects them to the port given in the AN\_Register message.

Rooster will try to use the sendfile(2) interface to connect the sockets together.

[![Rust](https://github.com/AnimaGUS-minerva/rooster/actions/workflows/rust.yml/badge.svg)](https://github.com/AnimaGUS-minerva/rooster/actions/workflows/rust.yml)




