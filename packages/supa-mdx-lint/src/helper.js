/**
 * Adapted from https://github.com/getsentry/sentry-cli
 * Under the [BSD 3-Clause License](https://github.com/getsentry/sentry-cli/blob/master/LICENSE)
 */

// @ts-check

"use strict";

const pty = require("node-pty");
const os = require("os");

const BINARY_DISTRIBUTIONS = [
  {
    packageName: "@supabase/supa-mdx-lint-darwin",
    subpath: "bin/supa-mdx-lint",
  },
  {
    packageName: "@supabase/supa-mdx-lint-linux-x64",
    subpath: "bin/supa-mdx-lint",
  },
  {
    packageName: "@supabase/supa-mdx-lint-linux-i686",
    subpath: "bin/supa-mdx-lint",
  },
  {
    packageName: "@supabse/supa-mdx-lint-linux-arm64",
    subpath: "bin/supa-mdx-lint",
  },
  {
    packageName: "@supabase/supa-mdx-lint-linux-arm",
    subpath: "bin/supa-mdx-lint",
  },
  {
    packageName: "@supabase/supa-mdx-lint-win32-x64",
    subpath: "bin/supa-mdx-lint.exe",
  },
  { packageName: "@supabase/cli-win32-i686", subpath: "bin/supa-mdx-lint.exe" },
];

function getDistributionForThisPlatform() {
  const arch = os.arch();
  const platform = os.platform();

  let packageName = undefined;
  if (platform === "darwin") {
    packageName = "@supabase/supa-mdx-lint-darwin";
  } else if (platform === "linux" || platform === "freebsd") {
    switch (arch) {
      case "x64":
        packageName = "@supabase/supa-mdx-lint-linux-x64";
        break;
      case "x86":
      case "ia32":
        packageName = "@supabase/supa-mdx-lint-linux-i686";
        break;
      case "arm64":
        packageName = "@supabase/supa-mdx-lint-linux-arm64";
        break;
      case "arm":
        packageName = "@supabase/supa-mdx-lint-linux-arm";
        break;
    }
  } else if (platform === "win32") {
    switch (arch) {
      case "x64":
      // Windows arm64 can run x64 binaries
      // @eslint-disable-next-line no-fallthrough
      case "arm64":
        packageName = "@supabase/supa-mdx-lint-win32-x64";
        break;
      case "x86":
      case "ia32":
        packageName = "@supabase/supa-mdx-lint-win32-i686";
        break;
    }
  }

  let subpath = undefined;
  switch (platform) {
    case "win32":
      subpath = "bin/supa-mdx-lint.exe";
      break;
    case "darwin":
    case "linux":
    case "freebsd":
      subpath = "bin/supa-mdx-lint";
      break;
    default:
      subpath = "bin/supa-mdx-lint";
      break;
  }

  return { packageName, subpath };
}

/**
 * Throws an error with a message stating that supa-mdx-lint doesn't support the current platform.
 *
 * @returns {never} nothing. It throws.
 */
function throwUnsupportedPlatformError() {
  throw new Error(
    `Unsupported operating system or architecture! supa-mdx-lint does not work on this architecture.

supa-mdx-lint supports:
- Darwin (macOS)
- Linux and FreeBSD on x64, x86, ia32, arm64, and arm architectures
- Windows x64, x86, and ia32 architectures`,
  );
}

/**
 * Tries to find the installed supa-mdx-lint binary - either by looking into the relevant
 * optional dependencies or by trying to resolve the fallback binary.
 *
 * @returns {string} The path to the supa-mdx-lint binary
 */
function getBinaryPath() {
  if (process.env.SUPA_MDX_LINT_BINARY_PATH) {
    return process.env.SUPA_MDX_LINT_BINARY_PATH;
  }

  const { packageName, subpath } = getDistributionForThisPlatform();

  if (packageName === undefined) {
    throwUnsupportedPlatformError();
  }

  try {
    return require.resolve(`${packageName}/${subpath}`);
  } catch {
    const otherInstalledDistribution = BINARY_DISTRIBUTIONS.find(
      ({ packageName, subpath }) => {
        try {
          require.resolve(`${packageName}/${subpath}`);
          return true;
        } catch {
          return false;
        }
      },
    );

    // These error messages are heavily inspired by esbuild's error messages: https://github.com/evanw/esbuild/blob/f3d535262e3998d845d0f102b944ecd5a9efda57/lib/npm/node-platform.ts#L150
    if (otherInstalledDistribution) {
      throw new Error(`supa-mdx-lint binary for this platform/architecture not found!

The "${otherInstalledDistribution.packageName}" package is installed, but for the current platform, you should have the "${packageName}" package installed instead. This usually happens if the "supa-mdx-lint" package is installed on one platform (for example Windows or MacOS) and then the "node_modules" folder is reused on another operating system (for example Linux in Docker).

To fix this, avoid copying the "node_modules" folder, and instead freshly install your dependencies on the target system. You can also configure your package manager to install the right package. For example, yarn has the "supportedArchitectures" feature: https://yarnpkg.com/configuration/yarnrc/#supportedArchitecture.`);
    } else {
      throw new Error(`supa-mdx-lint binary for this platform/architecture not found!

It seems like none of the "@supabase/supa-mdx-lint" package's optional dependencies got installed. Please make sure your package manager is configured to install optional dependencies. If you are using npm to install your dependencies, please don't set the "--no-optional", "--ignore-optional", or "--omit=optional" flags. supa-mdx-lint needs the "optionalDependencies" feature in order to install its binary.`);
    }
  }
}

class LinterError extends Error {
  constructor(message, code) {
    super(message);
    this.code = code;
  }
}

/**
 * Runs `supa-mdx-lint` with the given command line arguments.
 *
 * @example
 * const output = await execute(['--version']);
 * expect(output.trim()).toBe('supa-mdx-lint x.y.z');
 *
 * @param {string[]} args Command line arguments passed to `supa-mdx-lint`.
 * @returns {Promise<void>} A promise that resolves to the standard output.
 */
async function execute(args) {
  const env = { ...process.env };
  return new Promise((resolve, reject) => {
    const ptyProcess = pty.spawn(getBinaryPath(), args, {
      name: "xterm-color",
      cols: 80,
      rows: 30,
      cwd: process.cwd(),
      env: env,
    });

    ptyProcess.onData((data) => {
      process.stdout.write(data);
    });

    ptyProcess.onExit(({ exitCode: code }) => {
      if (code === 0) {
        resolve();
      } else {
        const error = new LinterError(
          `supa-mdx-lint exited with code ${code}`,
          code,
        );
        reject(error);
      }
    });

    if (process.stdin.isTTY) {
      process.stdin.setRawMode(true);
    } else {
      console.warn(
        "Warning: process.stdin is not a TTY. setRawMode is not available.",
      );
    }
    process.stdin.setEncoding("utf8");

    process.stdin.on("data", (data) => {
      const strData = data.toString();
      if (strData === "\u0003") {
        // Ctrl + C
        process.exit();
      }
      ptyProcess.write(strData);
    });
  });
}

module.exports = {
  execute,
};
