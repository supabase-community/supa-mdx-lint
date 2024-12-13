#/bin/bash

update_cargo_toml() {
  local file=$1
  local new_version=$2
  sed -i.bak -E "s/^version = \".*\"/version = \"$new_version\"/" "$file"
  rm "${file}.bak"
}

update_package_json() {
  local file=$1
  local new_version=$2
  jq --arg new_version "$new_version" '.version = $new_version' "$file" > tmp.$$.json && mv tmp.$$.json "$file"
}

update_optional_dependencies() {
  local file=$1
  local new_version=$2
  jq --arg new_version "$new_version" '.optionalDependencies |= with_entries(.value = $new_version)' "$file" > tmp.$$.json && mv tmp.$$.json "$file"
}

if ! command -v jq &> /dev/null
then
  echo "jq could not be found, please install it to proceed."
  exit
fi

if [ -z "$1" ];
then
  echo "Usage: $0 <new-version>"
  exit 1
fi

NEW_VERSION=$1

update_cargo_toml "Cargo.toml" "$NEW_VERSION"

update_package_json "npm-binary-distributions/darwin/package.json" "$NEW_VERSION"
update_package_json "npm-binary-distributions/linux-arm/package.json" "$NEW_VERSION"
update_package_json "npm-binary-distributions/linux-arm64/package.json" "$NEW_VERSION"
update_package_json "npm-binary-distributions/linux-i686/package.json" "$NEW_VERSION"
update_package_json "npm-binary-distributions/linux-x64/package.json" "$NEW_VERSION"
update_package_json "npm-binary-distributions/win32-i686/package.json" "$NEW_VERSION"
update_package_json "npm-binary-distributions/win32-x64/package.json" "$NEW_VERSION"
update_package_json "packages/supa-mdx-lint/package.json" "$NEW_VERSION"

update_optional_dependencies "packages/supa-mdx-lint/package.json" "$NEW_VERSION"

echo "Version updated to $NEW_VERSION in all manifest files."
