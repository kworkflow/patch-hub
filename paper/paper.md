---
title: ''
tags:
authors:
affiliations:
date:
bibliography: paper.bib

# Optional fields if submitting to a AAS journal too, see this blog post:
# https://blog.joss.theoj.org/2018/12/a-new-collaboration-with-aas-publishing
aas-doi: 
aas-journal:
---

# Summary

# Statement of need

# Introduction

The development of the Linux kernel is one of the foremost examples of a large-scale Free Source Software project. Dozens of subsystems and thousands of contributors have allowed Linux to continue evolving for decades, making it the foundation of most modern computing systems. This process follows a rigorous workflow of review and integration before a contribution — also known as a patch — can reach the software’s end users.

In general, the kernel development cycle is based on a repetition of tasks, both for contributors, who seek to have their code incorporated, and for maintainers, who must ensure that no issues are being introduced. In a simplified way, these tasks consist of compiling, running, and testing the Linux kernel, as well as organizing, sending, and responding to patches. In practice, this involves executing long sequences of verbose commands, which can take a significant amount of time to complete and must be repeated in every iteration of a contribution.

[ADD PATCH LIFECYCLE FIGURE]

Due to the repetitive nature of these tasks, it is common for kernel developers to create or adopt ad hoc scripts to automate such processes, in order to speed up execution and reduce the chance of errors. As a result, this tooling is generally decentralized. Such decentralization leads to duplicated efforts and may contribute to the lack of robust solutions for some of these tasks.

A Free Software tool that aims to mitigate this issue is Kworkflow (kw). Written in Bash, the software helps Linux kernel developers perform various tasks through a unified Command Line Interface (CLI). Among many other features, users can compile and deploy the kernel, as well as manage multiple custom configurations for different environments and use cases. In this way, kw directly addresses the main bottlenecks arising from repetitive tasks, allowing developers to focus on reviewing and contributing patches themselves.

Within kw, another notable functionality — which has become an independent utility and is the central topic of this paper — is patch-hub. With its own dedicated repository and implemented in Rust, patch-hub is a Terminal User Interface (TUI) focused on the interaction between developers/maintainers and the patchsets (groups of related patches representing a single contribution) of kernel subsystems. Each command in kw targets one or more specific tasks, and in the case of patch-hub, its goal is to simplify user interaction with mailing lists and the patchsets of each subsystem. Its main features include browsing subsystem mailing lists, viewing all patchsets within a list, and interacting with individual patchsets — such as applying one to the local kernel tree or saving a patch for later analysis. Under the hood, patch-hub takes advantage of Lore (lore.kernel.org), the public archive of the Linux kernel's mailing lists, which allows you to search for messages and patchsets on demand, in contrast to the traditional model based on subscription to the lists.

The remainder of this paper is organized as follows. First, we describe the tool in greater detail, covering its high-level functionality, architecture, and how it addresses certain kernel development bottlenecks. Next, we discuss the motivations and advantages of implementing it in Rust. Finally, we present the project’s next steps.

# patch-hub

## Features

In general, the main features of patch-hub are aligned with the goal presented in the previous section: to simplify the interaction between those involved in kernel development — contributors, reviewers, and maintainers — and the patchsets of each subsystem.

For end users of the tool, the most notable features are:

### Integration with mailing lists

Users can browse the mailing lists of each subsystem available on lore.kernel.org. For each list, they can navigate through the submitted patchsets — from the most recent to the oldest — and analyze each patchset individually.

[ADD LIST OF PATCHSETS SCREEN FIGURE]

### Patchset rendering

For every patchset, users can first view its metadata, which includes the title, author, patchset version, and the number of reviews, tests, or acknowledgments (acks) it has received. They can also inspect each individual patch within the patchset, as well as the patchset’s cover letter. For each patch, the commit message and the code diff can be viewed. This allows users to follow the full flow of who submitted the patch and to review each change introduced by the patchset individually.


[ADD COMPARISON OF RENDERS FIGURE]

### Patchset management

Beyond simply viewing patchsets, users can actively interact with them. Three main actions are supported:

1. Bookmarking a patchset to access it later.
2. Applying the patchset to a local kernel tree, to validate and test the proposed changes.
3. Replying to a patchset with a Reviewed-by trailer, to indicate that the patchset has been reviewed.

### Custom configuration

Another important feature is the ability to customize certain system settings. The main options include: selecting which tool will be used to render patchsets, configuring how many patchsets are displayed per page, defining directories for data and cache storage, and setting log retention periods. Users can also configure integration with Git commands: git send-email for replying to patchsets, and git am for applying a patchset to the local kernel tree. This ensures that the review and application workflow can be tailored to each user’s preferences.

[ADD CONFIGURATION SCREEN FIGURE]

## Architecture

The patch-hub architecture can be divided into two fundamental parts:

1. The core of the application, which handles all state changes triggered by user interaction.
2. The integration with lore.kernel.org, which provides access to up-to-date patchsets.

### MVC (Model–View–Controller)

The core of the application was developed following the Model–View–Controller (MVC) design pattern, where Model represents the aggregation of all application states, View represents the abstraction of how those states are rendered to the user, and Controller manages user interactions and requested actions.

#### Model

Practically speaking, patch-hub defines a struct named App, which implements the Model layer. In summary, it contains:

- A `CurrentScreen` attribute, indicating the application’s active screen.
- One struct for each possible screen the user can access (`MailingListSelection`, `BookmarkedPatchsets`, `LatestPatchsets`, `DetailsActions`, `EditConfig`).
- A `Config` struct that stores the current configuration.
- A `BlockingLoreAPIClient` struct that represents the HTTP client responsible for communicating with lore.kernel.org.

[ADD APP CODE SNIPPET]

As expected, the Model layer does not handle either end of the application — user interaction or terminal rendering — but only the core logic of the system. The `App` struct is responsible for storing each screen’s state, the loaded patchsets, configuration data, and for orchestrating transitions between states.
However, it does not directly handle user input or screen rendering.

#### View

Since patch-hub is a TUI, the View layer focuses on rendering each screen in the terminal. Concretely, whenever a state change occurs, the terminal is redrawn with the relevant information for the user, via the `draw_ui()` function. This function retrieves the current screen from the App and renders it according to its definition and current state. To draw widgets, patch-hub uses the Rust library Ratatui, which provides definitions for color, alignment, geometric shapes, and other UI components.

Besides rendering individual screens, some UI components — such as loading screens and pop-up windows — can appear across multiple views. Their rendering behavior is also handled within the View layer.

[ADD DRAW_UI CODE SNIPPET]

Notably, the View layer has no knowledge of how information is stored or which user interactions led to the current state. It only needs the current state to decide how to compose and display the interface elements.

#### Controller

The Controller layer coordinates the chain of operations triggered by user actions.
In general, it captures keyboard events and routes them to their corresponding actions, which typically involve an update to the App (Model), followed by a screen redraw (View). Each screen has its own event handler, and whenever a user action causes a screen transition, the corresponding handler function is invoked.

Thus, the Controller directly interacts with both the Model and the View, orchestrating their operation at a high level.

[ADD KEY -> ACTION ROUTING SNIPPET]

[ADD INTERACTION BETWEEN MODULES DIAGRAM]

### Lore API

With the MVC structure established, the other main pillar of patch-hub is its integration module with lore.kernel.org, which enables fetching patchsets. This module is not part of the MVC structure because it operates independently of the core business logic — it merely provides data to patch-hub, and could be replaced by another source without requiring changes elsewhere in the system.

The primary goal of this module is to communicate with lore.kernel.org through HTTP requests to retrieve the list and details of patchsets.

The HTTP client is represented by the `BlockingLoreAPIClient` struct, responsible for managing requests, handling network communication (headers, timeouts, etc.), and parsing the XML responses from the site.

Three main endpoints were implemented to provide all the information patch-hub needs about patches from lore.kernel.org:

- **request_available_lists**: returns all mailing lists of kernel subsystems available on lore.kernel.org;

- **request_patch_feed**: given a mailing list, returns the list of patchsets it contains;

- **request_patch_html**: given a patchset ID, returns the complete information about it and its contents.

The integration module also includes another struct, LoreSession, which acts as an intermediary between the App screens and the external data source. It is responsible for calling the `BlockingLoreAPIClient`, processing XML responses, storing loaded patchsets along with their associated mailing lists, and providing this data to the App whenever required.

This design keeps the external data source decoupled from the core application logic, facilitating both testing and potential future replacement or extension of how patchsets are retrieved.

[ADD REQUEST TO LORE FUNCTION SNIPPET]

## Discussion

### Rust

### Importance

A recurring concern among Linux kernel developers in recent years has been the sustainability of the development cycle, especially considering the bottlenecks created by the project’s scale combined with outdated development processes. One possible way to address these challenges — as discussed in [CITATION] — is through the increasing use of development support tools, which can help reduce the cognitive and operational burden of tasks that are secondary to the system’s evolution itself.

In the context of interacting with patches, users must understand the dynamics of mailing lists and learn the steps and conventions involved in patch submission and review. These factors can slow down the development cycle and make it harder to integrate new contributors, reviewers, and maintainers.

Patch-hub is one such support tool that aims to directly improve this scenario. By allowing users to visualize, validate, and respond to patchsets more quickly, intuitively, and in a centralized manner, the tool eliminates or simplifies many of the steps traditionally required in the process.

The lore.kernel.org platform itself is an example of a tool designed to simplify how users interact with patchsets. patch-hub builds on this well-established system, extending its functionality and usability so that users need nothing beyond their terminal to work with patchsets.

For these reasons, patch-hub can be viewed as a bridge between the traditional practices of kernel development — which depend on tools and technologies that are increasingly uncommon in modern software engineering — and more contemporary approaches that emphasize user experience as a means to boost productivity and reduce the likelihood of errors. Furthermore, when considered within the broader context of its integration with kw, patch-hub can significantly expand the potential for automation and, consequently, accelerate the entire development workflow.

# Acknowledgements

# References