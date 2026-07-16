# Contributing to Rustty

This document describes the process of contributing to Rustty. It is intended
for anyone considering opening an **issue** or **pull request**. For people who
are interested in developing Rustty and technical details behind it, please
check out our ["Developing Rustty"](HACKING.md) document as well.

Rustty is a solo, full-fidelity port of [Ghostty](https://github.com/ghostty-org/ghostty)
from Zig to Rust. Its design, features, and terminal semantics are those of
upstream Ghostty; the port's job is to reproduce them faithfully, not to
redesign them. Because of this, Rustty has no community-governance machinery of
its own: there is no vouch list, denouncement list, Discord server, or
discussion-category triage here. Questions about the terminal's design or
feature direction belong upstream at
[ghostty.org](https://ghostty.org) and the
[Ghostty project](https://github.com/ghostty-org/ghostty). For Rustty itself,
open an issue or pull request describing a concrete, actionable change to the
port — and only for code you fully understand.

## The Critical Rule

**The most important rule: the change has to be good.** It should build, pass
the test suite, be scoped, and stay consistent with the surrounding code and
with upstream Ghostty's semantics. That standard is the same whether a human,
an AI agent, or a fully autonomous pipeline produced it — we judge the work,
not the author.

## AI Usage

Rustty embraces AI. Use whatever tools you like, a human in the loop is
optional, and autonomous contributions are welcome. See the
[AI Usage Policy](AI_POLICY.md) for the details.

## General Patterns

### Issues are Actionable

The Rustty issue tracker is for _actionable items_.

Rustty **does not use the issue tracker for open-ended discussion or feature
requests**. Since Rustty is a faithful port, feature direction is decided
upstream in Ghostty; the Rustty issue tracker exists to track concrete,
well-understood work on the port itself. **This pattern makes it easier for
maintainers or contributors to find issues to work on since _every issue_ is
ready to be worked on.**

### Pull Requests Implement an Issue

Pull requests should be associated with a previously accepted issue.
**If you open a pull request for something that wasn't previously discussed,**
it may be closed or remain stale for an indefinite period of time. I'm not
saying it will never be accepted, but the odds are stacked against you.

Issues tagged with "feature" represent accepted, well-scoped work. If you
implement an issue as described, your pull request will be accepted with a high
degree of certainty.

> [!NOTE]
>
> **Pull requests are NOT a place to discuss feature design.** Please do
> not open a WIP pull request to discuss a feature. Instead, open an issue
> and link to your branch.
