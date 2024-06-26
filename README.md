# Flagrant - CLI-driven feature flagging system

Feature flagging niche is long time filled in with excellent solutions like [Unleash](https://www.getunleash.io/) or [Flagsmith](https://www.flagsmith.com/), so why yet another app on top of this cake? Flagrant has an ambition to become a Redis of feature flagging - small, reliable and completely CLI driven server providing all that's needed to keep features under control. Namely, it's supposed to bring in:

- multiple environments (prod, dev, test, etc)
- multivariant features
- segment overrides
- identity overrides
- scheduled on/off
- analytics
- client libraries for rust/jvm/js

As it's written in Rust, Flagrant comes with low-level resource utilisation and _blazingly fast_ mode switched on by default 😃

# Architecture
To keep things simple yet still allow for extensibilty, code has been structured into following modules (crates)

- flagrant - core logic (requests distribution among others)
- flagrant-types - core types shared across all other modules
- flagrant-api - HTTP server exposing crucial endpoints both for clients and management
- flagrant-cli - a command-line interface to communicate with server
- flagrant-client - a client library ensuring seamless communication between client- and server-side, with caching, etc.
 
