# Contributing to DF Studio

Thank you for helping. Bug reports, models that do not solve, documentation
fixes and pull requests are all welcome.

## Reporting a problem

Open an [issue](https://github.com/dafer238/dfstudio/issues/new/choose) and
include:

* the DF Studio version (shown at the bottom of the *Help* menu) and your
  operating system;
* the smallest model that shows the problem (a `.dfs` script or `.df`
  project), and what you expected;
* for a solve failure, the message in the status bar.

Please do not attach confidential models: reduce the model or replace the
numbers first.

## Development

```bash
git clone https://github.com/dafer238/dfstudio
cd dfstudio
cargo test --workspace
cargo run --release -p dfapp
```

Where things live is described in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md),
and the numerical methods in [docs/HOW_IT_WORKS.md](docs/HOW_IT_WORKS.md).

Before opening a pull request:

* run `cargo fmt --all` (the repository's `rustfmt.toml` sets
  the style; CI checks it), and `cargo clippy --workspace` should not
  report new warnings;
* `cargo test --workspace` passes; add a test for a bug fix or a new solver
  or property feature (`crates/dfcore/tests/` for end-to-end models);
* update the user guide or the language reference when behaviour visible to
  users changes, and add a line to `CHANGELOG.md` under *Unreleased*;
* numerical changes should cite their source (paper, standard or reference
  data) in a comment and in `docs/HOW_IT_WORKS.md`.

## Licence

DF Studio is licensed under the GNU Affero General Public License v3.0 or
later. By contributing, you agree that your contribution is licensed under
the same terms.
