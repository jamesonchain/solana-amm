import { readFileSync } from "node:fs";
import { expect } from "chai";
import {
  AccountRole,
  address,
  createClient,
  generateKeyPairSigner,
  getAddressEncoder,
  getProgramDerivedAddress,
  getU64Encoder,
  getUtf8Encoder,
  lamports,
} from "@solana/kit";
import type { Address, TransactionSigner } from "@solana/kit";
import {
  TOKEN_PROGRAM_ADDRESS,
  fetchMint,
  fetchToken,
  findAssociatedTokenPda,
  getCreateAssociatedTokenIdempotentInstructionAsync,
  getCreateMintInstructionPlan,
  getMintToATAInstructionPlanAsync,
} from "@solana-program/token";
import { litesvm } from "@solana/kit-plugin-litesvm";
import { generatedSigner } from "@solana/kit-plugin-signer";

// Read the built IDL so instruction bytes and account order match Anchor.
const ammIdl: {
  address: string;
  instructions: {
    name: string;
    discriminator: number[];
    accounts: { name: string; writable?: boolean; signer?: boolean }[];
    args: { name: string; type: "u64" | "u16" | "bool" }[];
  }[];
} = JSON.parse(readFileSync("target/idl/amm.json", "utf8"));
const programAddress = address(ammIdl.address);
const SYSTEM_PROGRAM_ADDRESS = address("11111111111111111111111111111111");

type IdlInstruction = (typeof ammIdl.instructions)[number];
type AccountAddresses = Record<string, Address>;
type InstructionSigners = Record<string, TransactionSigner>;
type InstructionArgument = bigint | boolean;

// Encode the generated Anchor IDL into a Kit instruction.
function makeInstruction(
  name: IdlInstruction["name"],
  accountAddresses: AccountAddresses,
  args: InstructionArgument[] = [],
  signers: InstructionSigners = {},
) {
  const definition = ammIdl.instructions.find(
    (instruction) => instruction.name === name,
  );
  if (!definition) {
    throw new Error(`Instruction ${name} is missing from the generated IDL`);
  }
  if (definition.args.length !== args.length) {
    throw new Error(`Wrong number of arguments for ${name}`);
  }

  // The IDL defines the discriminator and each argument's binary format.
  const encodedArguments = args.map((value, index) => {
    const type = definition.args[index].type;
    if (type === "u64") {
      if (typeof value !== "bigint") {
        throw new Error(`Argument ${definition.args[index].name} must be a bigint`);
      }
      return getU64Encoder().encode(value);
    }
    if (type === "u16") {
      if (typeof value !== "bigint" || value < 0n || value > 65_535n) {
        throw new Error(`Argument ${definition.args[index].name} must be a u16`);
      }
      const encoded = new Uint8Array(2);
      new DataView(encoded.buffer).setUint16(0, Number(value), true);
      return encoded;
    }
    if (typeof value !== "boolean") {
      throw new Error(`Argument ${definition.args[index].name} must be a boolean`);
    }
    return Uint8Array.of(value ? 1 : 0);
  });
  const data = new Uint8Array(
    definition.discriminator.length +
      encodedArguments.reduce((length, value) => length + value.length, 0),
  );
  data.set(definition.discriminator);
  let offset = definition.discriminator.length;
  for (const encoded of encodedArguments) {
    data.set(encoded, offset);
    offset += encoded.length;
  }

  // Account ordering and signer privileges come from the generated IDL.
  const accounts = definition.accounts.map((account) => {
    const accountAddress = accountAddresses[account.name];
    if (!accountAddress) {
      throw new Error(`Missing ${account.name} account for ${name}`);
    }
    const signer = signers[account.name];
    if (account.signer && !signer) {
      throw new Error(`Missing ${account.name} signer for ${name}`);
    }
    const role = account.signer
      ? account.writable
        ? AccountRole.WRITABLE_SIGNER
        : AccountRole.READONLY_SIGNER
      : account.writable
        ? AccountRole.WRITABLE
        : AccountRole.READONLY;
    return signer
      ? { address: accountAddress, role, signer }
      : { address: accountAddress, role };
  });

  return { programAddress, accounts, data };
}

// Compare the raw public key bytes to match Solana's Pubkey ordering.
function compareAddresses(left: Address, right: Address): number {
  const leftBytes = getAddressEncoder().encode(left);
  const rightBytes = getAddressEncoder().encode(right);
  for (let index = 0; index < leftBytes.length; index += 1) {
    if (leftBytes[index] !== rightBytes[index]) {
      return leftBytes[index] - rightBytes[index];
    }
  }
  return 0;
}

// Derive the addresses created by initialize from the documented PDA seeds.
async function derivePoolAddresses(mintA: Address, mintB: Address) {
  const encoder = getAddressEncoder();
  const utf8 = getUtf8Encoder();
  const addressSeed = (value: Address) => Uint8Array.from(encoder.encode(value));
  const textSeed = (value: string) => Uint8Array.from(utf8.encode(value));
  const derive = async (seeds: Uint8Array[]) => {
    const [derivedAddress] = await getProgramDerivedAddress({
      programAddress,
      seeds,
    });
    return derivedAddress;
  };
  const pool = await derive([
    textSeed("pool"),
    addressSeed(mintA),
    addressSeed(mintB),
  ]);
  const poolSeed = addressSeed(pool);
  const [lpMint, vaultA, vaultB, treasuryA, treasuryB] = await Promise.all([
    derive([textSeed("lp_mint"), poolSeed]),
    derive([textSeed("vault_a"), poolSeed]),
    derive([textSeed("vault_b"), poolSeed]),
    derive([textSeed("treasury_a"), poolSeed]),
    derive([textSeed("treasury_b"), poolSeed]),
  ]);
  return { pool, lpMint, vaultA, vaultB, treasuryA, treasuryB };
}

// Each test gets an isolated LiteSVM instance and fresh SPL token accounts.
async function createFixture(initializePool = true, feeBps = 100) {
  const client = await createClient().use(generatedSigner()).use(litesvm());
  const user = client.payer;
  client.svm.addProgramFromFile(programAddress, "target/deploy/amm.so");
  await client.airdrop(user.address, lamports(10_000_000_000n));

  const generatedMints = await Promise.all([
    generateKeyPairSigner(),
    generateKeyPairSigner(),
  ]);
  const [mintA, mintB] = generatedMints.sort((left, right) =>
    compareAddresses(left.address, right.address),
  );
  for (const mint of [mintA, mintB]) {
    await client.sendTransaction(
      await getCreateMintInstructionPlan(client, {
        payer: user,
        newMint: mint,
        decimals: 0,
        mintAuthority: user,
      }),
    );
    await client.sendTransaction(
      await getMintToATAInstructionPlanAsync({
        payer: user,
        owner: user.address,
        mint: mint.address,
        mintAuthority: user,
        amount: 1_000_000n,
        decimals: 0,
      }),
    );
  }

  const [userA] = await findAssociatedTokenPda({
    owner: user.address,
    tokenProgram: TOKEN_PROGRAM_ADDRESS,
    mint: mintA.address,
  });
  const [userB] = await findAssociatedTokenPda({
    owner: user.address,
    tokenProgram: TOKEN_PROGRAM_ADDRESS,
    mint: mintB.address,
  });
  const poolAddresses = await derivePoolAddresses(mintA.address, mintB.address);
  const emptyUserLp: Address = address("11111111111111111111111111111111");

  const fixture = {
    client,
    user,
    mintA: mintA.address,
    mintB: mintB.address,
    userA,
    userB,
    ...poolAddresses,
    userLp: emptyUserLp,
  };

  if (initializePool) {
    await client.sendTransaction(
      makeInstruction(
        "initialize",
        {
          payer: user.address,
          mint_a: fixture.mintA,
          mint_b: fixture.mintB,
          pool: fixture.pool,
          lp_mint: fixture.lpMint,
          vault_a: fixture.vaultA,
          vault_b: fixture.vaultB,
          treasury_a: fixture.treasuryA,
          treasury_b: fixture.treasuryB,
          token_program: TOKEN_PROGRAM_ADDRESS,
          system_program: SYSTEM_PROGRAM_ADDRESS,
        },
        [BigInt(feeBps)],
        { payer: user },
      ),
    );
    await client.sendTransaction(
      await getCreateAssociatedTokenIdempotentInstructionAsync({
        payer: user,
        owner: user.address,
        mint: fixture.lpMint,
      }),
    );
    const [userLp] = await findAssociatedTokenPda({
      owner: user.address,
      tokenProgram: TOKEN_PROGRAM_ADDRESS,
      mint: fixture.lpMint,
    });
    fixture.userLp = userLp;
  }

  return fixture;
}

// Add liquidity with the fixture user's token accounts.
async function deposit(
  fixture: Awaited<ReturnType<typeof createFixture>>,
  amountA: bigint,
  amountB: bigint,
  minLpOut: bigint,
) {
  await fixture.client.sendTransaction(
    makeInstruction(
      "deposit",
      {
        user: fixture.user.address,
        pool: fixture.pool,
        mint_a: fixture.mintA,
        mint_b: fixture.mintB,
        lp_mint: fixture.lpMint,
        vault_a: fixture.vaultA,
        vault_b: fixture.vaultB,
        user_a: fixture.userA,
        user_b: fixture.userB,
        user_lp: fixture.userLp,
        token_program: TOKEN_PROGRAM_ADDRESS,
      },
      [amountA, amountB, minLpOut],
      { user: fixture.user },
    ),
  );
}

// Expected failures must include the program error, not just any exception.
async function expectProgramFailure(
  action: () => Promise<unknown>,
  errorPattern: RegExp,
) {
  let failure: unknown;
  try {
    await action();
  } catch (error) {
    failure = error;
  }
  expect(failure, "Expected the instruction to fail").not.to.equal(undefined);
  expect(String(failure)).to.match(errorPattern);
}

describe("AMM LiteSVM", () => {
  it("initializes the pool, LP mint, reserve vaults, and treasuries", async () => {
    const fixture = await createFixture();

    expect(fixture.client.svm.getAccount(fixture.pool).exists).to.equal(true);
    expect((await fetchMint(fixture.client.rpc, fixture.lpMint)).data.decimals).to
      .equal(9);
    expect((await fetchToken(fixture.client.rpc, fixture.vaultA)).data.amount).to
      .equal(0n);
    expect((await fetchToken(fixture.client.rpc, fixture.vaultB)).data.amount).to
      .equal(0n);
    expect(
      (await fetchToken(fixture.client.rpc, fixture.treasuryA)).data.amount,
    ).to.equal(0n);
    expect(
      (await fetchToken(fixture.client.rpc, fixture.treasuryB)).data.amount,
    ).to.equal(0n);
  });

  it("accepts deposits and mints LP tokens", async () => {
    const fixture = await createFixture();
    await deposit(fixture, 100_000n, 100_000n, 100_000n);
    await deposit(fixture, 1_000n, 1_000n, 1_000n);

    expect((await fetchToken(fixture.client.rpc, fixture.vaultA)).data.amount).to
      .equal(101_000n);
    expect((await fetchToken(fixture.client.rpc, fixture.vaultB)).data.amount).to
      .equal(101_000n);
    expect((await fetchToken(fixture.client.rpc, fixture.userLp)).data.amount).to
      .equal(101_000n);
  });

  it("rejects a deposit below the minimum LP output", async () => {
    const fixture = await createFixture();

    await expectProgramFailure(
      () => deposit(fixture, 100_000n, 100_000n, 100_001n),
      /(SlippageExceeded|6004|0x1774)/i,
    );
    expect((await fetchToken(fixture.client.rpc, fixture.vaultA)).data.amount).to
      .equal(0n);
  });

  it("withdraws proportional reserves when LP tokens are burned", async () => {
    const fixture = await createFixture();
    await deposit(fixture, 100_000n, 100_000n, 100_000n);

    await fixture.client.sendTransaction(
      makeInstruction(
        "withdraw",
        {
          user: fixture.user.address,
          pool: fixture.pool,
          mint_a: fixture.mintA,
          mint_b: fixture.mintB,
          lp_mint: fixture.lpMint,
          vault_a: fixture.vaultA,
          vault_b: fixture.vaultB,
          user_lp: fixture.userLp,
          user_a: fixture.userA,
          user_b: fixture.userB,
          token_program: TOKEN_PROGRAM_ADDRESS,
        },
        [10_000n, 10_000n, 10_000n],
        { user: fixture.user },
      ),
    );

    expect((await fetchToken(fixture.client.rpc, fixture.userLp)).data.amount).to
      .equal(90_000n);
    expect((await fetchToken(fixture.client.rpc, fixture.vaultA)).data.amount).to
      .equal(90_000n);
    expect((await fetchToken(fixture.client.rpc, fixture.vaultB)).data.amount).to
      .equal(90_000n);
  });

  it("rejects a withdrawal with outputs below the minimum", async () => {
    const fixture = await createFixture();
    await deposit(fixture, 100_000n, 100_000n, 100_000n);

    await expectProgramFailure(
      () =>
        fixture.client.sendTransaction(
          makeInstruction(
            "withdraw",
            {
              user: fixture.user.address,
              pool: fixture.pool,
              mint_a: fixture.mintA,
              mint_b: fixture.mintB,
              lp_mint: fixture.lpMint,
              vault_a: fixture.vaultA,
              vault_b: fixture.vaultB,
              user_lp: fixture.userLp,
              user_a: fixture.userA,
              user_b: fixture.userB,
              token_program: TOKEN_PROGRAM_ADDRESS,
            },
            [10_000n, 10_001n, 10_000n],
            { user: fixture.user },
          ),
        ),
      /(SlippageExceeded|6004|0x1774)/i,
    );
    expect((await fetchToken(fixture.client.rpc, fixture.userLp)).data.amount).to
      .equal(100_000n);
  });

  it("rejects withdrawing more LP tokens than the user owns", async () => {
    const fixture = await createFixture();
    await deposit(fixture, 100_000n, 100_000n, 100_000n);

    await expectProgramFailure(
      () =>
        fixture.client.sendTransaction(
          makeInstruction(
            "withdraw",
            {
              user: fixture.user.address,
              pool: fixture.pool,
              mint_a: fixture.mintA,
              mint_b: fixture.mintB,
              lp_mint: fixture.lpMint,
              vault_a: fixture.vaultA,
              vault_b: fixture.vaultB,
              user_lp: fixture.userLp,
              user_a: fixture.userA,
              user_b: fixture.userB,
              token_program: TOKEN_PROGRAM_ADDRESS,
            },
            [100_001n, 0n, 0n],
            { user: fixture.user },
          ),
        ),
      /(InsufficientLiquidity|6003|0x1773)/i,
    );
  });

  it("swaps in both directions and sends half the fee to treasury", async () => {
    const fixture = await createFixture();
    await deposit(fixture, 100_000n, 100_000n, 100_000n);

    await fixture.client.sendTransaction(
      makeInstruction(
        "swap",
        {
          user: fixture.user.address,
          pool: fixture.pool,
          mint_a: fixture.mintA,
          mint_b: fixture.mintB,
          vault_a: fixture.vaultA,
          vault_b: fixture.vaultB,
          treasury_a: fixture.treasuryA,
          treasury_b: fixture.treasuryB,
          user_a: fixture.userA,
          user_b: fixture.userB,
          token_program: TOKEN_PROGRAM_ADDRESS,
        },
        [10_000n, 9_000n, true],
        { user: fixture.user },
      ),
    );
    expect((await fetchToken(fixture.client.rpc, fixture.treasuryA)).data.amount)
      .to.equal(50n);
    expect((await fetchToken(fixture.client.rpc, fixture.userB)).data.amount).to
      .equal(909_008n);
    expect((await fetchToken(fixture.client.rpc, fixture.vaultA)).data.amount).to
      .equal(109_950n);
    expect((await fetchToken(fixture.client.rpc, fixture.vaultB)).data.amount).to
      .equal(90_992n);

    await fixture.client.sendTransaction(
      makeInstruction(
        "swap",
        {
          user: fixture.user.address,
          pool: fixture.pool,
          mint_a: fixture.mintA,
          mint_b: fixture.mintB,
          vault_a: fixture.vaultA,
          vault_b: fixture.vaultB,
          treasury_a: fixture.treasuryA,
          treasury_b: fixture.treasuryB,
          user_a: fixture.userA,
          user_b: fixture.userB,
          token_program: TOKEN_PROGRAM_ADDRESS,
        },
        [10_000n, 1n, false],
        { user: fixture.user },
      ),
    );
    expect((await fetchToken(fixture.client.rpc, fixture.treasuryB)).data.amount)
      .to.equal(50n);
    expect((await fetchToken(fixture.client.rpc, fixture.userA)).data.amount).to
      .equal(900_788n);
    expect((await fetchToken(fixture.client.rpc, fixture.userB)).data.amount).to
      .equal(899_008n);
    expect((await fetchToken(fixture.client.rpc, fixture.vaultA)).data.amount).to
      .equal(99_162n);
    expect((await fetchToken(fixture.client.rpc, fixture.vaultB)).data.amount).to
      .equal(100_942n);
  });

  it("rejects a swap below the minimum output", async () => {
    const fixture = await createFixture();
    await deposit(fixture, 100_000n, 100_000n, 100_000n);

    await expectProgramFailure(
      () =>
        fixture.client.sendTransaction(
          makeInstruction(
            "swap",
            {
              user: fixture.user.address,
              pool: fixture.pool,
              mint_a: fixture.mintA,
              mint_b: fixture.mintB,
              vault_a: fixture.vaultA,
              vault_b: fixture.vaultB,
              treasury_a: fixture.treasuryA,
              treasury_b: fixture.treasuryB,
              user_a: fixture.userA,
              user_b: fixture.userB,
              token_program: TOKEN_PROGRAM_ADDRESS,
            },
            [10_000n, 10_000n, true],
            { user: fixture.user },
          ),
        ),
      /(SlippageExceeded|6004|0x1774)/i,
    );
    expect((await fetchToken(fixture.client.rpc, fixture.treasuryA)).data.amount)
      .to.equal(0n);
  });

  it("rejects a fee above the maximum", async () => {
    const fixture = await createFixture(false);

    await expectProgramFailure(
      () =>
        fixture.client.sendTransaction(
          makeInstruction(
            "initialize",
            {
              payer: fixture.user.address,
              mint_a: fixture.mintA,
              mint_b: fixture.mintB,
              pool: fixture.pool,
              lp_mint: fixture.lpMint,
              vault_a: fixture.vaultA,
              vault_b: fixture.vaultB,
              treasury_a: fixture.treasuryA,
              treasury_b: fixture.treasuryB,
              token_program: TOKEN_PROGRAM_ADDRESS,
              system_program: SYSTEM_PROGRAM_ADDRESS,
            },
            [1_001n],
            { payer: fixture.user },
          ),
        ),
      /(FeeTooHigh|6001|0x1771)/i,
    );
    expect(fixture.client.svm.getAccount(fixture.pool).exists).to.equal(false);
  });
});
