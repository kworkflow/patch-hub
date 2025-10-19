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

# Acknowledgements

# References