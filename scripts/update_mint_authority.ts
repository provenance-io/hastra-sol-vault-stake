import * as anchor from "@coral-xyz/anchor";
import {Program} from "@coral-xyz/anchor";
import {HastraSolVaultStake} from "../target/types/hastra_sol_vault_stake";
import {PublicKey} from "@solana/web3.js";
import yargs from "yargs";

const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);

const program = anchor.workspace.HastraSolVaultStake as Program<HastraSolVaultStake>;

const args = yargs(process.argv.slice(2))
    .option("mint", {
        type: "string",
        description: "Token that will have its mint authority set",
        required: true,
    })
    .option("new_authority", {
        type: "string",
        description: "Token that will have its mint authority set",
        required: true,
    })
    .parseSync();

const main = async () => {
    const [configPda, bump] = PublicKey.findProgramAddressSync([
        Buffer.from("config")
    ], program.programId);

    const [mintAuthorityPda] = anchor.web3.PublicKey.findProgramAddressSync(
        [Buffer.from("mint_authority")],
        program.programId
    );
    // bpf_loader_upgradeable program id
    const BPF_LOADER_UPGRADEABLE_ID = new PublicKey(
        "BPFLoaderUpgradeab1e11111111111111111111111"
    );
    // derive ProgramData PDA
    const [programData] = PublicKey.findProgramAddressSync(
        [program.programId.toBuffer()],
        BPF_LOADER_UPGRADEABLE_ID
    );

    const mint = new PublicKey(args.mint);
    const newAuthority = new PublicKey(args.new_authority);
    console.log("Mint (token to be minted):", mint.toBase58());
    console.log("New Authority:", newAuthority.toBase58());
    console.log("Config PDA:", configPda.toBase58());
    console.log("Mint Authority PDA:", mintAuthorityPda.toBase58());

    // Call set_mint_authority
    const tx = await program.methods
        .setMintAuthority(newAuthority)
        .accountsStrict({
            mint: mint,
            mintAuthority: mintAuthorityPda,
            config: configPda,
            signer: provider.wallet.publicKey,
            tokenProgram: anchor.web3.SystemProgram.programId,
            programData: programData,
        })
        .rpc();

    console.log("Transaction:", tx);
};

main().catch(console.error); 
