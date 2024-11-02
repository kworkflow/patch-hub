# `patch-hub`: _TUI for lore.kernel.org_

[![License: GPL v3+](https://img.shields.io/badge/license-GPL%20v2%2B-blue.svg)](https://www.gnu.org/licenses/old-licenses/gpl-2.0.html)

## :information_source: About

`patch-hub` is a TUI that streamlines the interaction with **software patches**
:wrench: sent through **mailing lists** :incoming_envelope: in the **development context**
:woman_technologist:. These mailing lists are archived on
[lore.kernel.org](https://lore.kernel.org), and their development context is the
Linux kernel and Linux-adjacent projects.

`patch-hub` is a sub-project of the `kw` _DAWS_ (Developer Automation Workflow
System). `kw` has a more straightforward (not simpler) mission. To quote from the `README`,
_"reduce the setup overhead of working with the Linux kernel and provide tools
to support developers in their daily tasks"_. Although it can be used as a
standalone tool, we recommend you use `patch-hub` as part of the `kw` tool
suite. Check out the [`kw` repository](https://github.com/kworkflow/kworkflow).

## :star: Features

<img src="assets/patch-hub-demo-v0.1.0.gif" width="100%" alt="patch-hub-demo-v0.1.0">

1. _Mailing List Selection_: Fetch the set of mailing lists archived on
[lore.kernel.org](https://lore.kernel.org) and visualize it dynamically.

2. _Latest Patchsets from_: Consult the flow of the latest patchsets from a
target mailing list.

3. _Patchset Details and Actions_: See details about patchsets, which include
the patchset metadata (title, author, version, number of total patches, last
updated, and so on) and individual patch contents, as well as apply actions
based on patchsets, like bookmarking/unbookmarking and replying to the entire
series with the `Reviewed-by` tag.

4. _Patchset Rendering_: Use external tools like [`bat`](https://github.com/sharkdp/bat),
[`delta`](https://github.com/dandavison/delta), [`diff-so-fancy`](https://github.com/so-fancy/diff-so-fancy)
to render the patch details and have a better preview screen with colors,
layouts and even syntax highlight. If you don't wish to use external
dependencies, you can opt for the vanilla renderer (denominated `default`).

5. _Bookmarked Patchsets_: Keep track of specific patchsets by bookmarking them
for later consult.

**More features coming!**

> [!NOTE]
> Actions like applying a patchset against a Linux kernel tree, compiling from
> this applied version, and installing it to a target machine (the last two are
> covered by the `kw` suite) will be progressively added.

## :package: How To Install

### external dependencies

In order to use `patch-hub` you must install:

- [`b4`](https://github.com/mricon/b4)
- [`git-email`](https://git-scm.com/docs/git-send-email) (`git send-email` command)
- [`bat`](https://github.com/sharkdp/bat), [`delta`](https://github.com/dandavison/delta) and/or [`diff-so-fancy`](https://github.com/so-fancy/diff-so-fancy) optional but highly recommended patchset renderers to enhance previewing patches

### pre-compiled binaries

You can find pre-compiled `patch-hub` binaries on our [releases page](https://github.com/kworkflow/patch-hub/releases).

There are two versions of the binary: `-x86_64-unknown-linux-gnu` (dynamically linked) and `-x86_64-unknown-linux-musl` (statically linked). The first uses the GNU C library (glibc), while the second uses the musl C library (musl libc). In a nutshell, `-x86_64-unknown-linux-gnu` is more compatible with various Linux distributions due to its reliance on the widely used glibc. At the same time, `x86_64-unknown-linux-musl` produces more portable and self-contained binaries because it includes all necessary libraries within the binary itself.

### compiling from source

If you wish to compile the project from source, just clone this repository in
your local machine and invoke

```
cargo build [--release]
```

to generate a `patch-hub` binary in `target/debug` (or `target/release`).

For this to work, you'll need to have `rustc` and `cargo` installed on your
system.

## :handshake: How To Contribute

We are still structuring an organized contribution process, but we more than
welcome proposed changes through Pull-Requests. For cataloged issues, you can
check our [issues page](https://github.com/kworkflow/patch-hub/issues).

In the meanwhile, you can check [`kw`'s contributor
guidelines](https://kworkflow.org/content/howtocontribute.html) for some
reference. Some things translate, due to `patch-hub` being a "child" of `kw`,
like the `master` and `unstable` branches scheme, but bear in mind that others
don't (e.g., `kw` is in Bash, while `patch-hub` is in Rust).
