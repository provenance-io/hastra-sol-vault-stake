import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { HastraSolVaultStake } from "../target/types/hastra_sol_vault_stake";
import {
  createMint,
  createAccount,
  mintTo,
  getAccount,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { assert } from "chai";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";

describe("hastra-sol-vault-stake", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.HastraSolVaultStake as Program<HastraSolVaultStake>;

  // Test accounts
  let vaultMint: PublicKey;
  let stakeMint: PublicKey;
  let vaultTokenAccount: PublicKey;
  let userVaultTokenAccount: PublicKey;
  let userStakeTokenAccount: PublicKey;
  let configPda: PublicKey;
  let vaultAuthorityPda: PublicKey;
  let mintAuthorityPda: PublicKey;
  let freezeAuthorityPda: PublicKey;
  let ticketPda: PublicKey;

  const user = provider.wallet;
  const freezeAdmin = Keypair.generate();
  const rewardsAdmin = Keypair.generate();
  const unbondingPeriod = 7 * 24 * 60 * 60; // 7 days in seconds

  before(async () => {
    // Airdrop SOL to test accounts
    await provider.connection.requestAirdrop(freezeAdmin.publicKey, 1000000000);
    await provider.connection.requestAirdrop(rewardsAdmin.publicKey, 1000000000);

    // Create vault mint (wYLDS)
    vaultMint = await createMint(
        provider.connection,
        user.payer,
        user.publicKey,
        null,
        6
    );

    // Create stake mint (PRIME)
    stakeMint = await createMint(
        provider.connection,
        user.payer,
        user.publicKey,
        null,
        6
    );

    // Create token accounts
    vaultTokenAccount = await createAccount(
        provider.connection,
        user.payer,
        vaultMint,
        user.publicKey
    );

    userVaultTokenAccount = await createAccount(
        provider.connection,
        user.payer,
        vaultMint,
        user.publicKey
    );

    userStakeTokenAccount = await createAccount(
        provider.connection,
        user.payer,
        stakeMint,
        user.publicKey
    );

    // Mint some vault tokens to user
    await mintTo(
        provider.connection,
        user.payer,
        vaultMint,
        userVaultTokenAccount,
        user.publicKey,
        1000000 // 1 token with 6 decimals
    );

    // Derive PDAs
    [configPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("config")],
        program.programId
    );

    [vaultAuthorityPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("vault_authority")],
        program.programId
    );

    [mintAuthorityPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("mint_authority")],
        program.programId
    );

    [freezeAuthorityPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("freeze_authority")],
        program.programId
    );

    [ticketPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("ticket"), user.publicKey.toBuffer()],
        program.programId
    );
  });

  it("Initializes the program", async () => {
    const tx = await program.methods
        .initialize(
            vaultMint,
            stakeMint,
            new anchor.BN(unbondingPeriod),
            [freezeAdmin.publicKey],
            [rewardsAdmin.publicKey]
        )
        .accounts({
          vaultTokenAccount: vaultTokenAccount,
          vaultMint: vaultMint,
          mint: stakeMint,
          signer: user.publicKey,
        })
        .rpc();

    // Verify config was created
    const config = await program.account.config.fetch(configPda);
    assert.equal(config.vault.toBase58(), vaultMint.toBase58());
    assert.equal(config.mint.toBase58(), stakeMint.toBase58());
    assert.equal(config.unbondingPeriod.toNumber(), unbondingPeriod);
    assert.equal(config.freezeAdministrators.length, 1);
    assert.equal(config.rewardsAdministrators.length, 1);
  });

  it("Deposits vault tokens and mints stake tokens", async () => {
    const depositAmount = new anchor.BN(100000); // 0.1 token

    const tx = await program.methods
        .deposit(depositAmount)
        .accounts({
          vaultTokenAccount: vaultTokenAccount,
          mint: stakeMint,
          signer: user.publicKey,
          userVaultTokenAccount: userVaultTokenAccount,
          userMintTokenAccount: userStakeTokenAccount,
        })
        .rpc();

    // Verify tokens were transferred and minted
    const vaultAccount = await getAccount(provider.connection, vaultTokenAccount);
    const userStakeAccount = await getAccount(provider.connection, userStakeTokenAccount);

    assert.equal(vaultAccount.amount.toString(), depositAmount.toString());
    assert.equal(userStakeAccount.amount.toString(), depositAmount.toString());
  });

  it("Creates unbonding ticket", async () => {
    const unbondAmount = new anchor.BN(50000); // 0.05 token

    const tx = await program.methods
        .unbond(unbondAmount)
        .accounts({
          signer: user.publicKey,
          mint: stakeMint,
          userMintTokenAccount: userStakeTokenAccount,
        })
        .rpc();

    // Verify ticket was created
    const ticket = await program.account.unbondingTicket.fetch(ticketPda);
    assert.equal(ticket.owner.toBase58(), user.publicKey.toBase58());
    assert.equal(ticket.requestedAmount.toString(), unbondAmount.toString());
    assert.isTrue(ticket.startTs.toNumber() > 0);
  });

  it("Fails to redeem before unbonding period", async () => {
    try {
      await program.methods
          .redeem()
          .accounts({
            vaultTokenAccount: vaultTokenAccount,
            signer: user.publicKey,
            userVaultTokenAccount: userVaultTokenAccount,
            userMintTokenAccount: userStakeTokenAccount,
            mint: stakeMint,
          })
          .rpc();

      assert.fail("Should have failed due to unbonding period not elapsed");
    } catch (error) {
      assert.include(error.toString(), "UnbondingPeriodNotElapsed");
    }
  });

  it("Creates rewards epoch", async () => {
    const epochIndex = new anchor.BN(1);
    const merkleRoot = Array(32).fill(0); // Mock merkle root
    const totalRewards = new anchor.BN(10000);

    const [epochPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("epoch"), epochIndex.toArrayLike(Buffer, "le", 8)],
        program.programId
    );

    const tx = await program.methods
        .createRewardsEpoch(epochIndex, merkleRoot, totalRewards)
        .accounts({
          admin: rewardsAdmin.publicKey,
        })
        .signers([rewardsAdmin])
        .rpc();

    // Verify epoch was created
    const epoch = await program.account.rewardsEpoch.fetch(epochPda);
    assert.equal(epoch.index.toString(), epochIndex.toString());
    assert.equal(epoch.total.toString(), totalRewards.toString());
  });

  it("Fails unauthorized freeze attempt", async () => {
    const unauthorizedUser = Keypair.generate();

    try {
      await program.methods
          .freezeTokenAccount()
          .accounts({
            tokenAccount: userStakeTokenAccount,
            mint: stakeMint,
            signer: unauthorizedUser.publicKey,
          })
          .signers([unauthorizedUser])
          .rpc();

      assert.fail("Should have failed due to unauthorized freeze administrator");
    } catch (error) {
      assert.include(error.toString(), "UnauthorizedFreezeAdministrator");
    }
  });

  it("Updates configuration", async () => {
    const newUnbondingPeriod = new anchor.BN(14 * 24 * 60 * 60); // 14 days

    // Get program data PDA for authority validation
    const BPF_LOADER_UPGRADEABLE_ID = new PublicKey(
        "BPFLoaderUpgradeab1e11111111111111111111111"
    );
    const [programData] = PublicKey.findProgramAddressSync(
        [program.programId.toBuffer()],
        BPF_LOADER_UPGRADEABLE_ID
    );

    const tx = await program.methods
        .updateConfig(newUnbondingPeriod)
        .accounts({
          programData: programData,
          signer: user.publicKey,
        })
        .rpc();

    // Verify configuration was updated
    const config = await program.account.config.fetch(configPda);
    assert.equal(config.unbondingPeriod.toString(), newUnbondingPeriod.toString());
  });

  it("Updates freeze administrators", async () => {
    const newFreezeAdmin = Keypair.generate();
    await provider.connection.requestAirdrop(newFreezeAdmin.publicKey, 1000000000);

    const BPF_LOADER_UPGRADEABLE_ID = new PublicKey(
        "BPFLoaderUpgradeab1e11111111111111111111111"
    );
    const [programData] = PublicKey.findProgramAddressSync(
        [program.programId.toBuffer()],
        BPF_LOADER_UPGRADEABLE_ID
    );

    const tx = await program.methods
        .updateFreezeAdministrators([freezeAdmin.publicKey, newFreezeAdmin.publicKey])
        .accounts({
          programData: programData,
          signer: user.publicKey,
        })
        .rpc();

    // Verify administrators were updated
    const config = await program.account.config.fetch(configPda);
    assert.equal(config.freezeAdministrators.length, 2);
  });

  it("Prevents too many administrators", async () => {
    const tooManyAdmins = Array(6).fill(0).map(() => Keypair.generate().publicKey);

    const BPF_LOADER_UPGRADEABLE_ID = new PublicKey(
        "BPFLoaderUpgradeab1e11111111111111111111111"
    );
    const [programData] = PublicKey.findProgramAddressSync(
        [program.programId.toBuffer()],
        BPF_LOADER_UPGRADEABLE_ID
    );

    try {
      await program.methods
          .updateFreezeAdministrators(tooManyAdmins)
          .accounts({
            programData: programData,
            signer: user.publicKey,
          })
          .rpc();

      assert.fail("Should have failed due to too many administrators");
    } catch (error) {
      assert.include(error.toString(), "TooManyAdministrators");
    }
  });

  it("Claims rewards with valid merkle proof", async () => {
    const epochIndex = new anchor.BN(2);
    const claimAmount = new anchor.BN(5000);

    // Create epoch first
    const [epochPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("epoch"), epochIndex.toArrayLike(Buffer, "le", 8)],
        program.programId
    );

    // Mock merkle proof - in real implementation, this would be computed off-chain
    const mockProof: number[][] = [];

    // For testing, create a simple merkle root that validates our claim
    const crypto = require('crypto');
    const userData = Buffer.concat([
      user.publicKey.toBuffer(),
      claimAmount.toArrayLike(Buffer, "le", 8),
      epochIndex.toArrayLike(Buffer, "le", 8)
    ]);
    const leafHash = crypto.createHash('sha256').update(userData).digest();

    await program.methods
        .createRewardsEpoch(epochIndex, Array.from(leafHash), claimAmount)
        .accounts({
          admin: rewardsAdmin.publicKey,
        })
        .signers([rewardsAdmin])
        .rpc();

    // Claim rewards
    const [claimRecordPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("claim"), epochPda.toBuffer(), user.publicKey.toBuffer()],
        program.programId
    );

    const tx = await program.methods
        .claimRewards(claimAmount, mockProof)
        .accounts({
          user: user.publicKey,
          epoch: epochPda,
          mint: stakeMint,
          userStakeTokenAccount: userStakeTokenAccount,
        })
        .rpc();

    // Verify claim record was created
    const claimRecord = await program.account.claimRecord.fetch(claimRecordPda);
    assert.isNotNull(claimRecord);
  });

  it("Prevents double claiming", async () => {
    const epochIndex = new anchor.BN(2);
    const claimAmount = new anchor.BN(5000);
    const mockProof: number[][] = [];

    const [epochPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("epoch"), epochIndex.toArrayLike(Buffer, "le", 8)],
        program.programId
    );

    const [claimRecordPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("claim"), epochPda.toBuffer(), user.publicKey.toBuffer()],
        program.programId
    );

    try {
      await program.methods
          .claimRewards(claimAmount, mockProof)
          .accounts({
            user: user.publicKey,
            epoch: epochPda,
            mint: stakeMint,
            userStakeTokenAccount: userStakeTokenAccount,
          })
          .rpc();

      assert.fail("Should have failed due to double claim attempt");
    } catch (error) {
      // Account already exists error indicates claim record prevents double claiming
      assert.isTrue(error.toString().includes("already in use") ||
          error.toString().includes("RewardsAlreadyClaimed"));
    }
  });
});
