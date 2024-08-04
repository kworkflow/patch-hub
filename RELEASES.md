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
