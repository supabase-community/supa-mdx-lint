import { describe, expect, it } from "vitest";

import { ruleTester } from "./utils.js";
import rule001 from "../rules/rule001_heading_case.js";

describe("Rule001HeadingCase", () => {
  it("should enforce sentence case headings", () => {
    const test = () => {
      ruleTester.run("Rule001HeadingCase", rule001, {
        valid: [
          {
            code: "# Sentence case heading",
          },
        ],
        invalid: [
          {
            code: "# all lowercase heading",
            errors: [
              {
                message: "Heading should be sentence case",
              },
            ],
          },
          {
            code: "# Title Case Heading",
            errors: [
              {
                message: "Heading should be sentence case",
              },
            ],
          },
        ],
      });
    };

    expect(test).not.toThrow();
  });
});