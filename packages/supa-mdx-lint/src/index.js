//@ ts-check

"use strict";

const helper = require("./helper");

async function main() {
  const args = process.argv.slice(2);
  await helper.execute(args);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

if (require.main === module) {
  main();
}
