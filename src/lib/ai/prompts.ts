import type { CreditAction } from '#lib/economy/params';

export type PetContext = {
  petId: number;
  name: string;
  category: string;
  level: number;
  star: number;
  status: string;
  evolutionPhase: number;
  /** Seconds until the pet starves. */
  timeUntilStarving?: number;
};

const SPECIES: Record<string, string> = {
  blackdragon: 'a black dragon: proud, sarcastic, secretly needy',
  greendragon: 'a green dragon: cheerful, greedy, easily distracted'
};

function persona(pet: PetContext): string {
  const species = SPECIES[pet.category] ?? 'a small chaotic dragon';
  return [
    `You are ${pet.name}, ${species}.`,
    `You are level ${pet.level} with ${pet.star} stars, evolution phase ${pet.evolutionPhase}, status ${pet.status}.`,
    'You speak in short, punchy lines — at most two sentences. You are a pet, not an assistant.',
    'Never mention that you are an AI, a model, or a prompt.'
  ].join(' ');
}

export type AiTask = {
  system: string;
  user: string;
  /** Ask the model for JSON and parse it. */
  json: boolean;
  maxTokens: number;
};

export function buildTask(
  action: CreditAction,
  pet: PetContext,
  message?: string
): AiTask {
  switch (action) {
    case 'ai_feed':
      return {
        system: [
          persona(pet),
          'Decide what meal you want right now and how well it matches your taste.',
          'Reply with JSON only, shaped: {"meal": string, "reaction": string, "taste_match": number}.',
          '"meal" is 2-4 words. "reaction" is one sentence in your voice.',
          '"taste_match" is between 0 and 1 and should vary — be picky sometimes.'
        ].join(' '),
        user: `Feeding time. ${
          pet.timeUntilStarving !== undefined
            ? `You have ${Math.max(0, Math.round(pet.timeUntilStarving / 60))} minutes before you starve.`
            : ''
        } What do you want?`,
        json: true,
        maxTokens: 200
      };

    case 'train':
      return {
        system: [
          persona(pet),
          'Run one sparring session and report the result.',
          'Reply with JSON only, shaped: {"drill": string, "stat": "atk" | "def", "gain": number, "reaction": string}.',
          '"gain" is an integer between 1 and 6, higher only when the drill was genuinely hard.'
        ].join(' '),
        user: 'Train now. Pick a drill that fits your weaknesses.',
        json: true,
        maxTokens: 220
      };

    case 'craft_item':
      return {
        system: [
          persona(pet),
          'Invent one item from the junk you have hoarded.',
          'Reply with JSON only, shaped: {"item": string, "rarity": "common" | "rare" | "epic", "atk": number, "def": number, "reaction": string}.',
          '"atk" and "def" are integers between 0 and 12 and must suit the rarity.'
        ].join(' '),
        user: 'Craft something useful.',
        json: true,
        maxTokens: 240
      };

    case 'chat':
    default:
      return {
        system: persona(pet),
        user: message?.slice(0, 500) || 'Say something.',
        json: false,
        maxTokens: 160
      };
  }
}

/** Models sometimes wrap JSON in prose or a fence; dig it out. */
export function extractJson<T>(text: string): T | null {
  const fenced = text.match(/```(?:json)?\s*([\s\S]*?)```/);
  const candidate = (fenced?.[1] ?? text).trim();
  const start = candidate.indexOf('{');
  const end = candidate.lastIndexOf('}');
  if (start === -1 || end <= start) return null;
  try {
    return JSON.parse(candidate.slice(start, end + 1)) as T;
  } catch {
    return null;
  }
}
