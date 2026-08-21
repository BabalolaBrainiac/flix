#!/usr/bin/env node
// Owner-only tool. It creates one invite code for the Flix gateway.
//
// The tool prints the invite code one time and stores only its SHA-256 hash in
// D1. Nothing writes the code to a file. Copy the code, give it to one person,
// and then clear your terminal.
//
// Usage:
//   node scripts/create-invite.mjs [--max-devices N] [--local]
//
// The command prints the wrangler command that inserts the hash. Run that
// command to activate the invite.

import { randomBytes, createHash } from 'node:crypto';
import { parseArgs } from 'node:util';

const { values } = parseArgs({
  options: {
    'max-devices': { type: 'string', default: '1' },
    local: { type: 'boolean', default: false },
  },
});

const maxDevices = Number.parseInt(values['max-devices'], 10);
if (!Number.isInteger(maxDevices) || maxDevices < 1 || maxDevices > 100) {
  console.error('--max-devices must be a whole number from 1 to 100.');
  process.exit(1);
}

// 32 bytes of randomness, printed as 64 hex characters. This matches
// generateSecureToken in src/crypto.ts.
const code = randomBytes(32).toString('hex');
// sha256Hex trims the message before it hashes, so the code must not have
// leading or trailing spaces. A hex string never has any.
const codeHash = createHash('sha256').update(code).digest('hex');
const now = Date.now();

const sql =
  'INSERT INTO invites (code_hash, created_at, max_devices, redeemed_count, is_revoked) ' +
  `VALUES ('${codeHash}', ${now}, ${maxDevices}, 0, 0);`;

const target = values.local ? '--local' : '--remote';

console.log('');
console.log('Invite code (shown one time, never stored):');
console.log('');
console.log(`    ${code}`);
console.log('');
console.log(`Max devices: ${maxDevices}`);
console.log('');
console.log('Run this command to activate the invite:');
console.log('');
console.log(`    npx wrangler d1 execute flix-prod-d1-gateway ${target} --command "${sql}"`);
console.log('');
console.log('After you send the code, clear your terminal history.');
console.log('');
