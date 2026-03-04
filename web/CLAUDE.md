# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

NEAR blockchain explorer frontend. React 18 + TypeScript + Tailwind CSS v4 + Vite. Uses react-router-dom for routing.

## Build & Development

- `yarn install` — install dependencies
- `yarn dev` — start dev server
- `yarn build` — typecheck and build (`tsc -b && vite build`)
- `yarn preview` — preview production build
- `npx tsc -b` — typecheck only (no linter configured)
- No test framework configured yet.

## Architecture

### Routing

Four routes defined in `src/App.tsx`, all wrapped in a shared `Layout`:
- `/` — Home (recent blocks list with pagination)
- `/block/:blockId` — Block detail with transaction list
- `/tx/:txHash` — Transaction detail with receipts
- `/account/:accountId` — Account transaction history

### API Layer (`src/api/`)

All API calls go through `src/api/client.ts` which POSTs to a fastnear backend (`VITE_API_BASE_URL` env var, defaults to `https://tx.main.fastnear.com`). Every endpoint is a POST to `/v0/{endpoint}` with a JSON body.

Endpoints in `src/api/endpoints.ts`: `getBlocks`, `getBlock`, `getTransactions`, `getAccount`.

### Two-Tier Transaction Model

Transactions have two representations with different detail levels:
- **`BlockTx` / `AccountTx`** — lightweight summary objects returned by block/account endpoints (flat fields like `is_success`, `is_relayed`, `real_signer_id`)
- **`TransactionDetail`** — full RPC-like nested structure with execution outcomes, receipts, and actions (returned by `getTransactions`)

`src/utils/parseTransaction.ts` normalizes `TransactionDetail` into a flat `ParsedTx`, including delegate/relayer detection.

### Custom Hooks

- `useTxDetails` — batches transaction hash arrays into `getTransactions` calls (max 20 per batch), returns a `Map<hash, ParsedTx>`. Used by block/account pages to lazily load full tx details.
- `usePagedCache` — cursor-based pagination with client-side page caching and background prefetching. Used for blocks list and account tx history.

### Widget System (`src/widgets/`)

Pluggable transaction detail widgets registered in `src/widgets/registry.ts`. Each widget has a `match(tx)` function and a `priority`. `getMatchingWidgets(tx)` returns matching widgets sorted by priority (highest first). Currently only a default widget exists.

### Display Components (`src/components/`)

Reusable components for rendering blockchain entities: `TransactionHash`, `AccountId`, `BlockHeight`, `BlockHash`, `NearAmount`, `GasAmount`, `TimeAgo`, `Action`, `TxRow`, `SearchBar`, `Pagination`, `Layout`.

### Utilities (`src/utils/`)

- `format.ts` — number/amount formatting helpers
- `time.ts` — timestamp utilities
- `parseTransaction.ts` — transaction normalization and delegate detection

## Conventions

- Use shared display components in `src/components/` for blockchain entities instead of inline JSX
- Tailwind utility classes directly in JSX; no CSS modules or separate stylesheets
- Default exports for pages and components
- Icons from `lucide-react`
- JSON display via `@uiw/react-json-view`
