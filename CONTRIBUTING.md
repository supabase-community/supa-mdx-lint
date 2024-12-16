# Contributing

## Releases

The release process requires three steps:

1. [Bump the version number](#bump-the-version-number)
2. [Create a release](#create-a-release)
3. [Publish npm packages](#publish-npm-packages)

### Bump the version number

Run `./scripts/bump-version.sh` to bump the version number. This updates version numbers in `package.json` and `Cargo.toml` files, and ensure that dependency versions match the new version number.

Usage:

```bash
./scripts/bump-version.sh <new-version-number>
```

Example:

```bash
./scripts/bump-version.sh 0.1.0
```

### Create a release

Use the GitHub UI to create a new release.

Once the release is created, the CI automatically builds artifacts for the new packages.

### Publish npm packages

The build workflow does not automatically publish npm packages. To publish the packages, run the `publish` workflow through workflow dispatch. Provide the Run ID of the corresponding build workflow as the input.
