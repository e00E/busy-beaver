# Introduction

This is a program that reproduces Bbchallenge's seed run. The code for the original seed run can be found [here](https://github.com/bbchallenge/bbchallenge-seed/). More context can be found on [Bbchallenge](https://bbchallenge.org/method).

The output of the seed run is a set of undecided turing machines. This program produces the same set. This confirms the original result.

# Running

Compile and run with `cargo run --release`.

The program uses all logical cores on the system. It regularly prints statistics while running. The output of the program is a human readable `log` file. It contains a line for all enumerated machines. Each line has the machine and a one character code for how it was classified : **h**alt, **l**oop, **u**ndecided, **i**rrelevant.

The statistics for a complete run are:

- halt: 34104723
- loop: 2711166
- undecided: 88664064
- irrelevant: 944579
- total: 126424532

The log file for a complete run thus contains 126424532 lines and is 4.7 GB large.

# Improvements

This program improves on the original seed run in some ways.

## Efficiency

On my desktop machine this program is at least two times faster than the seed run. I tested this by running both programs until they had enumerated 500 k machines, limited to 4 cores. The seed run took 47 minutes, this program 22 minutes. The relative speed likely increases with more cores.

I performed a full run on a virtual server with an AMD Milan Epyc 7003 CPU. The server had access to 24 cores, 48 threads. This run completed in 12 hours and used a peak of 3.2 GB memory.

## Interruption

This program can be gracefully interrupted while it is running. When ctrl-c is pressed, it saves its state to disk before quitting. On next start the program reads the previous state and continues from where it left off.

## Full log

The seed run logs only machines that are undecided. These machines form the database for Bbchallenge. This program logs all enumerated machines, not just the undecided ones. This gives more insight on the run.

# Architecture

TODO
