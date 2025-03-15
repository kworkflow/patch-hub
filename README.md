# `patch-hub`

[![License: GPL
v3+](https://img.shields.io/badge/license-GPL%20v2%2B-blue.svg)](https://www.gnu.org/licenses/old-licenses/gpl-2.0.html)

## :information_source: About

`patch-hub` is a terminal-based user interface (TUI) designed to simplify
working with software patches sent through mailing lists in Linux-related
development. It provides an efficient way to navigate and interact with patches
archived on [lore.kernel.org](https://lore.kernel.org), specifically for the
Linux kernel and adjacent projects.

`patch-hub` is a sub-project of `kw`, a Developer Automation Workflow System
(DAWS) for Linux kernel developers. The goal of `kw` is to "reduce the setup
overhead of working with the Linux kernel and provide tools to support
developers in their daily tasks."

While `patch-hub` can be used as a standalone tool, we highly recommend
integrating it with the full `kw` suite for a more seamless developer
experience. Check out the [kw
repository](https://github.com/kworkflow/kworkflow/) to learn more.

## :star: Features

<img src="assets/patch-hub-demo-v0.1.0.gif" width="100%"
alt="patch-hub-demo-v0.1.0">

### Core Features

- **Mailing List Selection** — Dynamically fetch and browse mailing lists
  archived on [lore.kernel.org](https://lore.kernel.org).

- **Latest Patchsets** — View the most recent patchsets from a selected mailing
  list in an organized flow.

- **Patchset Details & Actions** —  View individual patch contents and access
  metadata like title, author, version, number of patches, last update, and
  code-review trailers. Take quick actions like:
  - **Apply patch(set)** to your local kernel tree.
  - **Bookmark** important patches
  - **Reply with `Reviewed-by` tags** to the series.

- **Bookmarking System** — Bookmark patchsets for easy reference
  later.

- **Enhanced Patchset Rendering** — Use external tools such as
  [`bat`](https://github.com/sharkdp/bat),
  [`delta`](https://github.com/dandavison/delta),
  [`diff-so-fancy`](https://github.com/so-fancy/diff-so-fancy) for better
  visualization or use the built-in vanilla renderer (`default`) for a
  dependency-free experience.

### More Features Coming!

Future updates will introduce deeper integration with kw, including:

- Seamlessly compile and deploy patchset versions of the kernel to target
  machines.

## :package: How To Install

Before using `patch-hub`, make sure you have the following packages installed:

- [`b4`](https://github.com/mricon/b4) - Required for working with patches from
  mailing lists.
- [`git-email`](https://git-scm.com/docs/git-send-email) - Provides the `git
  send-email` command for replying to patches.
- **Optional (but recommended) patchset renderers for enhanced previews:**
  - [`bat`](https://github.com/sharkdp/bat)
  - [`delta`](https://github.com/dandavison/delta) 
  - [`diff-so-fancy`](https://github.com/so-fancy/diff-so-fancy)

### `patch-hub` in the `kw` suite

`patch-hub` is a part of `kw` so if you already use kw, you don’t need to
install Patch Hub separately—simply update kw to the latest version: 
```bash
kw self-update 
``` 
After updating `kw`, you can launch `patch-hub` using: 
```bash 
kw patch-hub
```
If you’re not using `kw` yet, consider installing it for a
more fluid experience with `patch-hub` and other useful tools included in `kw`.

### :inbox_tray: Pre-compiled binaries

Download pre-compiled binaries from our [releases
page](https://github.com/kworkflow/patch-hub/releases).

We provide two versions:

- `-x86_64-unknown-linux-gnu` – Dynamically linked, relies on the GNU C Library
  **(glibc)**, making it more compatible across Linux distributions.
- `-x86_64-unknown-linux-musl`– Statically linked, built with **musl libc**,
  producing a more portable and self-contained binary.

### :wrench: Compiling from Source

:pushpin: **Requirements:** Ensure **Rust** (`rustc`) and **Cargo** are
installed on your system.

To build `patch-hub` from source:

:one: Clone the repository: ```bash git clone
https://github.com/kworkflow/patch-hub.git && cd patch-hub ```

:two: Compile
with Cargo: ```bash cargo build --release ```

:three: The compiled binary will
be available at:
- `target/release/patch-hub`
- `target/debug/patch-hub` (default build, without the `--release` option)

Additionally, you can install the binary to make it available anywhere on your
system: ```bash cargo install --path . ```

## :handshake: How To Contribute

We appreciate all contributions, whether it’s fixing bugs, adding features, improving documentation, or suggesting ideas! If you're looking for something to work on, check out our [issues page](https://github.com/kworkflow/patch-hub/issues).

Since patch-hub is a sub-project of kw, some development workflows carry over — such as a similar branching strategy (master and unstable) — but keep in mind that patch-hub is written in Rust, while kw is Bash. For detailed guidelines on setting up your environment, coding standards, and submitting Pull Requests, please refer to our [CONTRIBUTING.md](./CONTRIBUTING.md).

### :pushpin: Quick Contribution Guide
- **Report Issues:** Found a bug or have a feature request? Open an issue [here](https://github.com/kworkflow/patch-hub/issues).
- **Submit a PR:** Fix an issue, improve code, or add a new feature—just follow the steps in [CONTRIBUTING.md](./CONTRIBUTING.md).
- **Improve Documentation:** Enhancements to the README, guides, and comments are always welcome!
