import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

// Issue #659: coven-docs (https://docs.opencoven.ai/) is the canonical public
// documentation source. The coven repository must NOT call its own local
// `docs/` tree "the canonical documentation suite". Its ownership/maintenance
// prose must point at the canonical site and reframe local docs/ as
// code-adjacent (kept beside code), not the public canonical suite.
//
// Constraint from the issue: do NOT remove local docs until content exists
// canonically upstream. So these tests only assert prose/ownership framing,
// never the removal of any local doc path (those remain asserted by
// cli-docs-test.mjs and onboarding-docs-test.mjs).

const CANONICAL_URL = 'https://docs.opencoven.ai';

function readRepoFile(path) {
  return readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
}

test('README no longer calls its local docs/ tree the canonical documentation suite', () => {
  const readme = readRepoFile('README.md');

  // The specific stale claim the issue targets (README "Key directories" bullet).
  assert.doesNotMatch(
    readme,
    /The canonical documentation suite\. All product docs/,
    'README must not describe the local docs/ tree as "The canonical documentation suite"',
  );

  // More generally: coven must not assert its own docs/ tree is *the* canonical
  // public documentation suite anywhere.
  assert.doesNotMatch(
    readme,
    /\bcanonical documentation suite\b/i,
    'README must not claim any local docs/ tree is the canonical documentation suite',
  );
});

test('README points to the canonical public docs site as canonical', () => {
  const readme = readRepoFile('README.md');
  assert.match(
    readme,
    new RegExp(CANONICAL_URL.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')),
    'README must link the canonical public documentation site',
  );
  // The Key directories entry for docs/ must reframe it as code-adjacent and
  // reference the canonical site instead of claiming to be it.
  assert.match(
    readme,
    /`docs\/`\*\*[\s\S]*?docs\.opencoven\.ai/,
    'README docs/ directory entry must reference the canonical docs site (docs.opencoven.ai)',
  );
});

test('DOCS-MAINTENANCE reframes local docs/ as code-adjacent, not the canonical public suite', () => {
  const maint = readRepoFile('docs/DOCS-MAINTENANCE.md');

  // Must name the canonical public source.
  assert.match(
    maint,
    new RegExp(CANONICAL_URL.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')),
    'DOCS-MAINTENANCE must name the canonical public docs site',
  );
  assert.match(
    maint,
    /coven-docs/,
    'DOCS-MAINTENANCE must name the coven-docs canonical repository',
  );

  // Must no longer flatly assert these are "public product docs" as the
  // canonical public source; the canonical public source is coven-docs.
  assert.doesNotMatch(
    maint,
    /^Docs in this repository are public product docs\.$/m,
    'DOCS-MAINTENANCE must not assert local docs/ are the public product docs source',
  );

  // Must state code-adjacent framing.
  assert.match(
    maint,
    /code-adjacent/i,
    'DOCS-MAINTENANCE must describe local docs/ as code-adjacent',
  );
});

test('README resources table roadmap link uses the canonical public docs site', () => {
  const readme = readRepoFile('README.md');
  // The resources table Public Roadmap entry must not point at a local
  // docs/ROADMAP.md page while docs.opencoven.ai is the canonical site.
  assert.doesNotMatch(
    readme,
    /\[\*\*Public Roadmap\*\*\]\(docs\/ROADMAP\.md\)/,
    'Public Roadmap resource link must use the canonical docs site, not local docs/ROADMAP.md',
  );
});
