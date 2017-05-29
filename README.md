# Snow

[![Crates.io](https://img.shields.io/crates/v/snow.svg)](https://crates.io/crates/snow)
[![Docs.rs](https://docs.rs/snow/badge.svg)](https://docs.rs/snow)
[![Build Status](https://travis-ci.org/mcginty/snow.svg?branch=master)](https://travis-ci.org/mcginty/snow)

![totally official snow logo](http://i.imgur.com/XVakFvn.jpg)

An implementation of the [Noise Protocol](https://noiseprotocol.org/) by Trevor Perrin that is designed to be
Hard To Fuck Up™.

See the documentation at [https://docs.rs/snow](https://docs.rs/snow).

🔥 This library is in the state of **preview** - do everyone a favor and only use this for fun or criticizing the author's code for now.

## Implemented

Snow is currently tracking and working on fully implementing the **revision 32** specification.

- [x] Rekey()
- [x] `pskN` modifier
- [ ] `fallback` modifier

## Crypto
Cryptographic providers are swappable through `NoiseBuilder::with_provider()`, but by default it chooses select, artisanal
pure-Rust implementations (see `Cargo.toml` for a quick overview).

### Acceleration

If you enable the `ring-accelerated` feature, Snow will default to choosing `ring`'s *much* faster crypto implementations when supported.

If you enable the `ring-resolver` feature, Snow will include a ring_wrapper module as well as a `RingAcceleratedResolver` available to be used with `NoiseBuilder::with_resolver()`.

## What's it look like?
See `examples/simple.rs` for a more complete TCP client/server example.

```rust
let noise = NoiseBuilder::new("Noise_NN_ChaChaPoly_BLAKE2s".parse().unwrap())
                         .build_initiator()
                         .unwrap();
 
let mut buf = [0u8; 65535];
 
// write first handshake message
noise.write_message(&[0u8; 0], &mut buf).unwrap();
 
// receive response message
let incoming = receive_message_from_the_mysterious_ether();
noise.read_message(&incoming, &mut buf).unwrap();
 
// complete handshake, and transition the state machine into transport mode
let noise = noise.into_transport_mode();
```

## Status

Work in progress. Unreviewed. Unaudited. All APIs are unstable. Don't use for security critical purposes. *Side effects may include nausea, heart palpatations, yolocryptosis, and permanent sexual dysfunction.*
