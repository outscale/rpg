# RPG - Remote PacketGraph

This is a quite simple api of [packetgraph](https://github.com/outscale/packetgraph) library written in [Rust](https://www.rust-lang.org/).

This is a quite a beta at the moment with a limited number of available bricks
to connect each others.
For it's first version, you can create switch, firewall, hub, nic and tap network bricks. You can interconnect those bricks inside a graph which runs a separate thread.

# API Client & Documentation

You can read a [generated version](https://osu.eu-west-2.outscale.com/jerome.jutteau/rpg/index.html) of API documentation. However, you can generate clients for many languages by importing `swagger.yaml` file in [swagger's online editor](http://editor.swagger.io).

# Run RPG
You have two ways of running rpg:
- Use a pre-built portable binary for linux in [release page](https://github.com/outscale/rpg/releases)
- Build RPG from scratch (see next section).

Once you have your rpg binary, you can tweek some [ENV variables](https://rocket.rs/guide/overview/#environment-variables) when running it or setup `Rocket.toml`:
```
$ ROCKET_ENV=production ./rpg
```

You can also pass some DPDK options using `PG_DPDK_OPTS`:
```
$ ROCKET_ENV=production PG_DPDK_OPTS="-c1 -n1 --no-huge" ./rpg
```

# Build RPG from scratch

### Build and install packetgraph

Go to [Packetgraph](https://github.com/outscale/packetgraph/) instructions to build it.
Once you have built Packetgraph, you can install it on your system using `make install`.

### Install Rust 
You will need to install rust nightly in order to build RPG.
In a nutshell:
```
$ curl https://sh.rustup.rs -sSf | sh
$ rustup toolchain install nightly
$ rustup default nightly
```

### Build RPG
In rpg folder, run `cargo build --release`. If you installed packetgraph in a specific folder using `--prefix`, you probably want to set `LIBRARY_PATH`, `LD_LIBRARY_PATH` and `C_INCLUDE_PATH` in your build command.

Once built, rpg binary is located in `target/release/rpg`.

# License: GPLv3

> Copyright 2017 Outscale SAS
>
> RPG is free software: you can redistribute it and/or modify
> it under the terms of the GNU General Public License version 3 as published
> by the Free Software Foundation.
>
> Packetgraph is distributed in the hope that it will be useful,
> but WITHOUT ANY WARRANTY; without even the implied warranty of
> MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
> GNU General Public License for more details.
>
> You should have received a copy of the GNU General Public License
> along with Packetgraph.  If not, see <http://www.gnu.org/licenses/>.
