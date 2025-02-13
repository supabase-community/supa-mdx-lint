#!/usr/bin/env node

//@ ts-check

"use strict";

const helper = require("./helper");

async function main() {
  const args = process.argv.slice(2);
  try {
    await helper.execute(args);
    process.exit(0);
  } catch (err) {
    process.exit(err.code ?? 1);
  }
}

if (require.main === module) {
  main();
}
