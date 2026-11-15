# Project workflow

The user requested a Rust reimplementation of Yuka System and a runnable demo.
The GitHub repository is https://github.com/kesmeey/YUka-Rs.git.

After each completed, validated improvement, commit the relevant source changes
and push them to the configured upstream. The user explicitly authorized this
ongoing commit-and-push workflow. Keep each commit focused on a working step.
Run the applicable tests, `cargo fmt --check`, and Clippy before committing.

This directory has its own Git repository. Run project Git commands here.
Use the existing branch and inspect remote changes before pushing.

Game inputs are supplied separately. Keep original archives, game executables,
extracted media, local reverse-engineering evidence, downloaded references, and
build output in their existing ignored locations. The demo uses these inputs
read-only. Retain the existing license and third-party notices.

Document supported behavior and verified native semantics precisely. Record
approximations in the runtime report and relevant documentation. Unsupported
script operations should report their script position and stop execution.
