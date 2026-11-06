import { describe, expect, it } from 'vitest';
import { feedSummary, type AiFeedResult } from './client';

function result(parts: Partial<AiFeedResult>): AiFeedResult {
  return {
    action: 'ai_feed',
    creditsSpent: 0.002,
    meal: 'charred sardine',
    reaction: 'Acceptable.',
    tasteMatch: 0.8,
    lensApplied: false,
    xpMultiplier: 1.64,
    xpGained: 57,
    xpAwarded: true,
    xpTxHash: 'HASH',
    xpError: null,
    tokens: 180,
    ...parts
  };
}

describe('feedSummary', () => {
  it('reports the XP when it reached the chain', () => {
    expect(feedSummary(result({}))).toBe('+57 XP (1.64x)');
  });

  it('mentions the lens, since the player paid for it', () => {
    expect(feedSummary(result({ lensApplied: true, xpMultiplier: 2.05, xpGained: 72 }))).toBe(
      '+72 XP (2.05x, lens)'
    );
  });

  // The whole reason this function exists. For as long as nothing called
  // `award_xp`, every feed showed "+63 XP" and the pet stayed at `xp: 0`: the
  // player saw a level that was never coming and no hint why.
  it('does not claim XP the chain never got', () => {
    const summary = feedSummary(
      result({ xpAwarded: false, xpTxHash: null, xpError: 'Only the feed oracle can award XP' })
    );

    expect(summary).not.toContain('+57');
    expect(summary).toBe('No XP written on chain — Only the feed oracle can award XP');
  });

  it('keeps a long contract error inside a toast', () => {
    const summary = feedSummary(result({ xpAwarded: false, xpError: 'x'.repeat(200) }));
    expect(summary.length).toBeLessThan(110);
    expect(summary.endsWith('…')).toBe(true);
  });

  it('has something to say when the failure came with no reason', () => {
    expect(feedSummary(result({ xpAwarded: false, xpError: null }))).toContain(
      'the award did not go through'
    );
  });
});
