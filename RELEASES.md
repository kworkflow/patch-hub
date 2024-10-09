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
