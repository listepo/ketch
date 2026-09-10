//! Conventional Commit rules shared by local hooks and CI.

// Commit format is conventional commits, exactly as config-conventional
// defines them: the default type enum already covers every type this repo
// uses, and release-plz reads the same `feat:`/`fix:`/`!`/`BREAKING CHANGE:`
// grammar to decide the version bump. Local rules would drift from both, so
// there are none for the format.
//
// The one local rule is authorship, which config-conventional has no opinion
// on. AGENTS.md makes the human the only author, while an agent's harness
// adds a Co-Authored-By trailer or a "Generated with …" line by default — so a
// rule the agent has to remember becomes one the commit has to pass. Only an
// emoji may precede "Generated", so prose that quotes the phrase still passes.
// The optional U+FE0F is the variation selector that makes an emoji render
// in colour; it is invisible, so it is spelled as an escape.
const ATTRIBUTION = [
  /^co-authored-by:.*$/im,
  /^[ \t]*(?:\p{Extended_Pictographic}\u{FE0F}?[ \t]*)?generated (?:with|by)\b.*$/imu,
];

export default {
  extends: ["@commitlint/config-conventional"],
  plugins: [
    {
      rules: {
        "no-agent-attribution": ({ raw = "" }) => {
          const line = ATTRIBUTION.map((re) => raw.match(re)?.[0]).find(Boolean);
          return [!line, `the human is the only author (AGENTS.md); remove: ${line}`];
        },
      },
    },
  ],
  rules: { "no-agent-attribution": [2, "always"] },
};
