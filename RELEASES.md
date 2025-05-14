# Version 0.1.6 (2025-05-16)

After almost three months, a new release of `patch-hub` is here! We had a couple of new contributors who participated in this development cycle. Welcome @lucasoshiro and @ivinjabraham!

Besides being a long time since the last release, what motivated me the most to make this one is that we are looking to start making some breaking refactorings to the codebase.

### New Functionalities and Changes Highlight

1. Fix the preview bug of showing the `--` that marks the start of patches as a deletion
2. Fix `delta` output width 
3. Add new CI workflows
4. Add contributing guidelines
5. Fix invisible cursor bug when panic occurred during a loading screen
6. Redesign of the config file loading and updating
7. Lots of refactors and other fixes

### Problems and Future Changes

The jump to the next level of patch-hub seems closer than ever. For the last few months, I haven't been able to dedicate the time I desired to the project, and I aim to change gears and get back on top of things as before.

### Changelog

```git
96d56c5 2025-05-16 | fix(release): disable CI job for release based on `cargo dist` [David Tadokoro]
cc0f47d 2025-05-14 | fix(dependabot): correct invalid `applies-to` syntax in groups config [David Tadokoro]
e12cadf 2025-04-26 | docs: link test and corresponding function in `loading_screen` [Ivin Joel Abraham]
6a2adf6 2025-04-26 | docs: link tests and corresponding functions for `utils` [Ivin Joel Abraham]
5b98a5e 2025-04-26 | refactor: rename test function to be more consistent with the rest [Ivin Joel Abraham]
5c33f94 2025-04-26 | docs: link tests and corresponding functions for `patch_renderer` [Ivin Joel Abraham]
b2558bc 2025-04-26 | docs: link tests and corresponding functions for `Config` [Ivin Joel Abraham]
696346b 2025-04-25 | chore: move `test_samples/` directory out of `src/` [Ivin Joel Abraham]
adf9766 2025-04-01 | chore: bump vulnerable dependencies and adapt `ureq::Agent::user_agent()` call [Rafael Passos]
ccec50c 2025-03-23 | feat: add unit tests for serde_individual_default proc_macro [Lorenzo Bertin Salvador]
3a99675 2025-03-23 | feat: create proc_macro to simplify Config attributes deserialization [Lorenzo Bertin Salvador]
b286857 2025-03-23 | refactor: use lazy_static to store only one Config::default() object in memory [Lorenzo Bertin Salvador]
6760e79 2025-03-23 | feat: prevent existing config to be overwritten when new attribute is created [Lorenzo Bertin Salvador]
af8d1ef 2025-03-23 | refactor: improve code quality in config.rs module [Lorenzo Bertin Salvador]
65d96d4 2025-04-07 | refactor(config-tests): use untracked temporary sample config file [David Tadokoro]
66d3059 2025-03-21 | chore: delete redundant test [Ivin Joel Abraham]
7238f16 2025-03-19 | perf: resolve arguments before building `App` [Ivin Joel Abraham]
f9156f3 2025-03-19 | refactor: inject `App` with `Config` instead of building `Config` internally [Ivin Joel Abraham]
2ec6fdc 2025-03-16 | ci: add workflow for cargo install verification [Ivin Joel Abraham]
c4daa29 2025-03-15 | fix: ensure loading_screen! generated thread joins before exiting patch-hub [Lorenzo Bertin Salvador]
a00a27b 2025-03-15 | docs: add `CONTRIBUTING.md` with contribution guidelines [Ivin Joel Abraham]
548e0b5 2025-03-15 | docs: improve clarity and add more instructions to `README.md` [Ivin Joel Abraham]
ac214c0 2025-03-15 | fix: `cargo install` failing to build when using `ureq v3.0.9` [Ivin Joel Abraham]
faf2937 2025-02-26 | feat: add dependabot integration [Ivin Joel Abraham]
2957eef 2025-02-21 | fix: improve error handling in config management [Ivin Joel Abraham]
9c2c520 2025-02-20 | fix: handle missing patch IDs gracefully in feed pagination [Ivin Joel Abraham]
99337cf 2025-02-20 | fix: improve error handling for stdin in patch renderers [Ivin Joel Abraham]
a71ef6e 2025-02-20 | refactor: replace `unwrap()` with `expect()` in `toggle_action` [Ivin Joel Abraham]
8dc2a65 2025-02-20 | refactor: improve error handling in `App::new` and `Logger::init_log_file` [Ivin Joel Abraham]
b93c09a 2025-02-20 | fix: handle missing $HOME variable gracefully [Ivin Joel Abraham]
7b8cdd6 2025-02-20 | fix: properly handle `unwrap()`s in `reply_patchset_with_reviwed_by` [Ivin Joel Abraham]
b0b65ee 2025-03-15 | refactor: improve error handling and logging for config loading [Ivin Joel Abraham]
4acf6f8 2025-02-20 | refactor: improve `apply_patchset` validation and structure [Ivin Joel Abraham]
2c01ffa 2025-03-07 | chore: remove testing branch from `build_and_unit_test` CI workflow [David Tadokoro]
60e1920 2025-03-07 | feat: make CI workflows run on the exact PR branch [David Tadokoro]
31718ea 2025-03-11 | feat: disable gitconfig reading by Delta [Lucas Oshiro]
7dc9f78 2025-02-25 | feat: increase Delta output width [Lucas Oshiro]
68686e9 2025-03-05 | chore: remove leftover `foo.patch` file [David Tadokoro]
38c1c59 2025-02-25 | test: add setup function and new test cases for `utils` hooks [Ivin Joel Abraham]
1d0826a 2025-02-21 | fix: trim trailing `--` from patches before previewing [Ivin Joel Abraham]
```

# Version 0.1.5 (2025-02-19)

The time between releases grows, and the number of changes per release decreases... This is true, but it is also true that version 0.1.5 comes with long-awaited features!

### New Functionalities and Changes Highlight

1. Sending replies with `Reviewed-by` for a subset of patches.
2. Easily consult the number of `Reviewed-by`, `Acked-by`, and `Tested-by`.
3. Patchset application with configurations for multiple kernel trees (great job from @OJarrisonn).
4. Pop-ups for help screens, info messages, and much more (another great one from @OJarrisonn).

### Problems and Future Changes

Now the idea is to stop the adding of features and stabilize the project. We still have poor unit-test coverage and the architecture is showing signs of turning into a big ball of mud... In this respect, we are planning a major redesign of `patch-hub` to make it better for developers and users alike!

# Version 0.1.4 (2024-11-19)

It has become a staple of the project at this point, but this release took a long time to be done (longer than v0.1.2 to v.0.1.3). As like in v0.1.3, this release consists of a mix of new features, refactorings and some fixes. Another cycle with great contributions from @OJarrisonn, @th-duvanel, and @lorenzoberts. Many thanks and welcome for the new contributors @auyer and @gabrielsrd. There are major new changes already on the oven as opened Pull-Requests, but I found it worth to make a new release, both to mark the progress done and to not have a huge time gap between releases.

### New Functionalities and Changes Highlight

1. Add motions for patch previewing, allowing user to scroll half/full pages, go to start/end, and pan horizontally. Also in this context, add fullscreen preview.
2. Add dynamic loading screen with spinner for delayed actions.
3. Add new CLI option `-c | --show-configs` to display current configuration JSON.
4. Finish extraction refactoring of UI module.
5. Create lib crate `lore` to house all domain specific implementation and make codebase more organized.
6. Fix annoying and mysterious flakyness of Lore API requests by replacing `reqwest` client for a `ureq` one.
7. Fix poor performance of patch previews by pre-rendering them when opening patchsets.
8. Fix overall unreasonable high CPU usage by disabling auto-refresh of UI.

### Problems and Future Changes

Mostly, there were developments in many diverse fronts so (I think) the technical debt is being paid, whilst new developments are being introduced.

Besides that, as previously mentioned, there are 3 major changes opened as PRs that are closer to merging than not. However, there are still some polishing and validating of those (understanble, due to their impact), so these will likely enter in the next release. On this note, the next release might be the last one before v.0.2.0, i.e., a beta version of `patch-hub` :eyes:.

# Version 0.1.3 (2024-10-09)

Following the trend of v.0.1.2, this release took a long time to be done, and even more commits (forty!). These consisted of a mix of new features, refactorings and some fixes. This cycle was mostly done by @OJarrisonn, @th-duvanel, and @lorezonberts. My work was predominantly about solving merge conflicts and bugging on them (sorry guys :grimacing:). There are many optimazations and advances that aren't covered in the section below, but the highlights are described.

### New Functionalities and Changes

1. Add CI workflow for formatting and linting and enforce those in the codebase.
2. Add a logging system with garbage collection.
3. Use `derive_getters` instead of exhaustively implementing getters.
4. Add many configurations and make them editable through the screen "Edit Config".
5. Add CI workflow for building and testing.
6. Modularize and encapsulate a great deal of the Model (`src/app.rs`) and Controller (`src/handler.rs`) components.
7. Add support for alternative patch renderers bat, delta, and diff-so-fancy.

### Problems and Future Changes

This release opened up many possibilities for us to go going forward. Below are some of them.

1. Continue to expand on the refactor front, mainly by modularizing and revamping the View component.
2. Finish refining the patch preview screen is also something to keep on the radar.
3. Adding a decent UI to replying patchsets, as well as expanding its functionalities (replying to a subset of patches, using other tags, in-line reviewing, etc.).
4. There is already some talk about implementing a dynamic loading screen (which in itself is a huge rabbit hole).
5. Finally, trying to solve the flaky behavior of Lore API requests.

# Version 0.1.2 (2024-09-06)

It has been quite a while since the release of v0.1.1 (about one month). The rate of development dropped a little, but there were some meaningful contributions (including from new contributors :stuck_out_tongue:), so this release is to avoid keeping `patch-hub` outdated.

### New Functionalities and Changes

1. Add a `--version | -V` flag to display the `patch-hub` version (issue #17).
2. Suppress warning for unused function `centerd_rect`.
3. Rearrange unit test sample files to `src/test_samples` and add unit tests for `src/mailing_lists.rs` and `src/lore_api_client`.
4. Create the cache directory before using it (issue #21).
5. Remove vendoring of `openssl` (issue #22).
6. Remove tab completion from the "Mailing List Selection" screen and allow the user to input the list without entirely typing it (issue #25).
7. Fix crashing bug of hitting `ENTER` when there is no bookmarked patchset (issue #12).
8. Fix overflow in the patchsets of the last page.

### Problems and Future Changes

Many fronts are being explored right now, like: making configurations more robust and adding `git send-email` configurations, adding formatting and linting to the project, implementing logging, and more. The integration of `kw` and `patch-hub` is also being done by [this PR](https://github.com/kworkflow/kworkflow/pull/1155).

IMHO, the following steps are to close these started fronts, make a new release, and then stop adding features to pay the enormous technical debt we have in the project.

# Version 0.1.1 (2024-08-07)

This is a minor release to fix the issues #18 and #19.

### New Functionalities and Changes

1. Use a vendored version of the `openssl` dependency to ensure that the pre-compiled binaries of `patch-hub` don't crash at start-up like mentioned in #19 and to avoid missing dependencies when cross-compiling for the `x86_64-unknown-linux-musl` target.
2. Add support to the `x86_64-unknown-linux-musl` target, which produces more portable and self-contained binaries when compared with the `-gnu` target. This was done by adding this target to the release CI pipeline.

### Problems and Future Changes

Adding `openssl` as a vendored dependency seems like an acceptable decision to solve the issues mentioned, which are kind of critical (especially #19). Still, the drawbacks of longer compile times, bigger executables, and more memory usage ("duplicated" `openssl` objects) aren't neglectable, so other solutions should be considered.

# Version 0.1.0 (2024-08-04)

I am really happy to announce the **first** release of `patch-hub` :tada: :confetti_ball: :sparkles:

This is a big release, as I wanted to have a somewhat stable vertical prototype for `patch-hub`.

### New Functionalities and Changes

The big picture of features provided by this release of `patch-hub` are:

1. On-demand fetching the available mailing list archived on `lore.kernel.org` and dynamically displaying them based on the buffered user input (i.e., the available lists shown have the user input as a prefix) for the target mailing list. This comprises the _Mailing List Selection_ screen, the first screen displayed when running `patch-hub`.

2. Consulting the flow of the latest patchsets from a target mailing list. This comprises the _Latest Patchsets from_ screen, displayed after the user selects a target mailing list in the _Mailing List Selection_ screen.

3. Seeing the details of individual patchsets, which include the patchset metadata (title of the representative patch, version of the series, number of total patches, time of update on Lore servers...) and visualization of all the messages in the series, as well as applying actions on them. This comprises the _Patchset Details and Actions_ screen, displayed after the user selects a specific patchset (either in the `Latest Patchsets from` or `Bookmarked Patchsets` screen). Currently, there are two actions on patchsets provided: bookmarking/unbookmarking and replying to the full series with the `Reviewed-by` tag.

4. Keep track of specific patchsets that were bookmarked for future consultation. This comprises the _Bookmarked Patchsets_ screen, which displays the patchsets that had the bookmark action applied and is accessible through the _Mailing List Selection_ screen by hitting the `F1` key. This screen is practically identical to _Latest Patchsets from_, and selecting a patchset from this screen results in the _Patchset Details and Actions_ screen.

**Note**: The reply with the `Reviewed-by` tag provided by the _Patchset Details and Actions_ screen is set to run with the `--dry-run` for validation purposes.  When we feel comfortable, we will remove this flag (ideally, making it configurable).

### Problems and Future Changes

The project is still in the prototype stage for this first release, as the UI and functionality will probably mutate a lot. In terms of the project architecture, overall structure, and robustness, it will also mutate a lot: 

1. The "non-library components" of the project don't have any tests implemented and have minimal to no modularity (the _Controller_ component is wholly defined in `src/main.rs`).

2. There is no formatting nor linting enforced. Linting may not be necessary (or even possible) as the Rust compiler keeps us really safe, and the community doesn't seem to usually adopt extra linting/sanitizing tools.

3. There are many potential unhandled errors due to the massive use of `unwrap()` and the like that can cause ugly crashes.

4. Having the motto "early optimization is the root of all evil" in mind, I didn't focus on the most efficient algorithms; despite their efficiency being more than acceptable (at the moment), we may need to address some potential unnecessary bottlenecks.

5.  The reply with the `Reviewed-by` tag provided by the _Patchset Details and Actions_ screen has a barebones visualization, as its user input and displayed output don't use the project UI (i.e., it isn't done using [`ratatui`](https://ratatui.rs/)).
