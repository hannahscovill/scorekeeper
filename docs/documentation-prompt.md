The openapi spec doesn't match the entity definittions listed below.
Let's go one openapi yaml file at a time.

Please make the Game in docs/openapi/components/schemas/game.yaml match the typescript Game interface below

#### Entity Definitions

```typescript
interface Move {
  letter: string & { length: 1 }
  letter_contained_in_answer: boolean
  correct_letter_and_position: boolean
}
type Guess = [Move, Move, Move, Move, Move]

interface Game {
  game_id: string          // uuid
  user_id: string          // uuid
  puzzle_id: string        // uuid (e.g., "wordle-2026-01-18")
  moves: [Guess, Guess?, Guess?, Guess?, Guess?, Guess?] // 1-6 guesses
  moves_qty: number        // count of guesses
  completed_at_millis: number
  won: boolean
}

interface Team {
  team_id: string          // uuid
  team_name: string
  created_at_millis: number
}

interface TeamMembership {
  team_id: string
  user_id: string
  role: 'admin' | 'member'
  joined_at_millis: number
}

interface Scoreboard {
  scoreboard_id: string    // uuid
  team_id: string          // uuid
  puzzle_id: string        // which puzzle this scoreboard is for
}

interface Puzzle {
  puzzle_id: string        // uuid even though cant we just use the date? Do we really need this?
  word: string
  date_iso_day: string     // date formatted like YYYY-MM-DD
}
```