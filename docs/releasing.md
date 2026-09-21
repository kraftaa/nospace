# Releasing `nospace`

The Python distribution is named `nospace-cli`; the installed executable is
still named `nospace`. The `nospace` name on PyPI belongs to an unrelated
project and must not be used.

## One-time PyPI setup

1. Create the public GitHub repository that will contain this standalone
   project.
2. In PyPI, create a pending trusted publisher for `nospace-cli` with:
   - the GitHub owner and repository for this project;
   - workflow name `python-wheels.yml`;
   - environment name `pypi`.
3. In the GitHub repository settings, create a `pypi` environment. Protect it
   with required reviewers if desired.

The workflow uses GitHub's OIDC identity. Do not create or store a long-lived
PyPI API token in the repository.

## Validate without publishing

Run the `Python wheels` workflow manually. The workflow builds two wheels and
installs the manylinux x86-64 wheel in a clean Python environment before
running `nospace --version`. Manual runs never publish.

The release matrix contains:

- manylinux 2.28 x86-64;
- manylinux 2.28 ARM64;

No source distribution is uploaded. Consequently, pip never falls back to
compiling Rust when a user's platform is unsupported.

## Publish

1. Update the version in `Cargo.toml`.
2. Update `Cargo.lock`, the changelog and documentation.
3. Run the full safe and privileged test suites.
4. Commit the release.
5. Create and push an exact matching tag such as `v0.1.0`.

The tag starts the workflow and publishes only after every wheel builds and the
x86-64 installation smoke test succeeds. PyPI versions cannot be overwritten;
fixes require a new version.
