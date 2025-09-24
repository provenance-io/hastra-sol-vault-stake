#!/bin/bash

prompt_with_default_no_history() {
  local varname="$1"
  local prompt="$2"
  local default="${!varname}"
  read -p "$prompt [$default]: " input
  if [ -n "$input" ]; then
    eval "$varname=\"$input\""
  fi
}

prompt_with_default_no_history SOLANA_NETWORK "Select Solana network (devnet, mainnet-beta, testnet)"

HISTORY_FILE="${SOLANA_NETWORK}_vault.history"

update_history_var() {
  local varname="$1"
  local value="${!varname}"
  sed -i.bak "/^$varname=/d" "$HISTORY_FILE"
  echo "$varname=\"$value\"" >> "$HISTORY_FILE"
}

prompt_with_default() {
  local varname="$1"
  local prompt="$2"
  local default="${!varname}"
  read -p "$prompt [$default]: " input
  if [ -n "$input" ]; then
    eval "$varname=\"$input\""
  else
    eval "$varname=\"$default\""
  fi
  update_history_var "$varname"
}

# Load previous selections if history file exists
if [ -f "$HISTORY_FILE" ]; then
  source "$HISTORY_FILE"
fi

# Ensure history file exists
if [ ! -f "$HISTORY_FILE" ]; then
  touch "$HISTORY_FILE"
fi


show_current_settings() {
  echo ""
  echo "Current settings in $HISTORY_FILE:"

  # read each line of $HISTORY_FILE and print variable name and value in a tabbed format and order by variable name
  sort "$HISTORY_FILE" | while IFS='=' read -r name value; do
    # remove quotes from value
    value=$(echo "$value" | tr -d '"')
    printf "  %-30s %s\n" "$name:" "$value"
  done
  echo ""

}

show_current_settings

case "$SOLANA_NETWORK" in
  devnet) SOLANA_URL="https://api.devnet.solana.com" ;;
  mainnet-beta) SOLANA_URL="https://api.mainnet-beta.solana.com" ;;
  testnet) SOLANA_URL="https://api.testnet.solana.com" ;;
  *) echo "Invalid network"; exit 1 ;;
esac

# get the keypair from solana config
CONFIG_FILE="$HOME/.config/solana/cli/config.yml"
if [ -f "$CONFIG_FILE" ]; then
  SOLANA_KEYPAIR=$(grep 'keypair_path:' "$CONFIG_FILE" | awk '{print $2}')
  if [ -z "$KEYPAIR" ]; then
    KEYPAIR="$SOLANA_KEYPAIR"
    update_history_var "KEYPAIR"
  fi
fi

prompt_with_default KEYPAIR "Enter path to Solana wallet keypair"

if [ -z "$VAULT_MINT" ]; then
  prompt_with_default VAULT_MINT "Enter Vault Token Mint address (the token accepted for swap)"
fi

export ANCHOR_PROVIDER_URL="$SOLANA_URL"
export ANCHOR_WALLET="$KEYPAIR"

create_mint_token() {
  echo "Creating Mint Token..."
  MINT_TOKEN=$(spl-token create-token --decimals 6 --enable-freeze \
    --url "$SOLANA_URL" \
    --config "$CONFIG_FILE" | grep -oE 'Address:  ([A-Za-z0-9]+)' | awk '{print $NF}')
  echo "Mint Token: $MINT_TOKEN"
  sed -i.bak "/^MINT_TOKEN=/d" "$HISTORY_FILE"
  echo "MINT_TOKEN=\"$MINT_TOKEN\"" >> "$HISTORY_FILE"
}

create_vault_token_account() {
  echo "Creating Vault Token Account (ATA)..."
  VAULT_TOKEN_ACCOUNT=$(spl-token create-account "$VAULT_MINT" \
    --owner "$KEYPAIR" \
    --url "$SOLANA_URL" | grep -oE 'Creating account.* ([A-Za-z0-9]+)' | awk '{print $NF}')
  echo "Vault Token Account: $VAULT_TOKEN_ACCOUNT"
  sed -i.bak "/^VAULT_TOKEN_ACCOUNT=/d" "$HISTORY_FILE"
  echo "VAULT_TOKEN_ACCOUNT=\"$VAULT_TOKEN_ACCOUNT\"" >> "$HISTORY_FILE"
}

set_new_program_id() {
  rm ../target/deploy/hastra_sol_vault_stake-keypair.json
  # generate new keypair for program
  PROGRAM_ID=$(solana-keygen new --no-passphrase --no-outfile | grep -oE 'pubkey: ([A-Za-z0-9]+)' | awk '{print $NF}')
  echo "New Program ID: $PROGRAM_ID"
  PROGRAM_FILE="../programs/hastra-sol-vault-stake/src/lib.rs"
  # Update lib.rs declare_id:
  sed -i '' "s/declare_id!(\"[A-Za-z0-9]*\");/declare_id!(\"$PROGRAM_ID\");/" $PROGRAM_FILE
  echo "Updated ${PROGRAM_FILE} with new Program ID"
  echo "Cleaning and Building..."
  anchor clean
  build_program
}

copy_idl_types() {
  local default_type_dest="../../hastra-fi-nexus-flow/src/types/hastra-sol-vault-stake.ts"
  local default_idl_dest="../../hastra-fi-nexus-flow/src/types/idl/hastra-sol-vault-stake.ts"
  echo ""
  read -p "Enter destination for hastra_sol_vault_stake.ts TYPE [$default_type_dest]: " dest_type
  read -p "Enter destination for hastra_sol_vault_stake.ts IDL  [$default_idl_dest]: " dest_idl
  echo ""
  dest_type="${dest_type:-$default_type_dest}"
  dest_idl="${dest_idl:-$default_idl_dest}"
  cp ../target/types/hastra_sol_vault_stake.ts "$dest_type"
  echo "Copied to $dest_type"
  cp ../target/idl/hastra_sol_vault_stake.json "$dest_idl"
  # add TS const to top of IDL file
  sed -i '' '1s/^/export const HastraSolVaultStake = /' "$dest_idl"
  echo "Copied to $dest_idl"
}
build_program() {
  anchor build
  copy_idl_types
}

deploy_program() {
  echo "Deploying Program..."
  echo "Getting Program ID..."
  PROGRAM_ID=$(solana-keygen pubkey ../target/deploy/hastra_sol_vault_stake-keypair.json)
  update_history_var "PROGRAM_ID"
  # Update lib.rs declare_id:
  PROGRAM_FILE="../programs/hastra-sol-vault-stake/src/lib.rs"
  sed -i '' "s/declare_id!(\"[A-Za-z0-9]*\");/declare_id!(\"$PROGRAM_ID\");/" $PROGRAM_FILE
  echo "Updated ${PROGRAM_FILE} with new Program ID ${PROGRAM_ID}"
  echo "Saving Deploy Keypair to local config ${HOME}/.config/solana"
  cp ../target/deploy/hastra_sol_vault_stake-keypair.json $HOME/.config/solana

  build_program

  solana program deploy ../target/deploy/hastra_sol_vault_stake.so \
    --url "$SOLANA_URL" \
    --keypair "$KEYPAIR" \
    --config "$CONFIG_FILE"
  echo "Program deployed with ID: $PROGRAM_ID"
}

build_and_deploy() {
  build_program
  deploy_program
}

initialize_program() {
  if [ -z "$FREEZE_ADMINISTRATORS" ]; then
    prompt_with_default FREEZE_ADMINISTRATORS "Enter comma-separated list of Freeze Administrator addresses"
  fi
  if [ -z "$REWARDS_ADMINISTRATORS" ]; then
    prompt_with_default REWARDS_ADMINISTRATORS "Enter comma-separated list of Rewards Administrator addresses"
  fi

  if [ -z "$UNBONDING_PERIOD" ]; then
    prompt_with_default UNBONDING_PERIOD "Enter Unbonding Period (in seconds)"
  fi

  if [ -z "$MINT_TOKEN" ]; then
    prompt_with_default MINT_TOKEN "Enter Mint Token (staking token minted) address"
  fi

  if [ -z "$VAULT_TOKEN_ACCOUNT" ]; then
    prompt_with_default VAULT_TOKEN_ACCOUNT "Enter Vault (token deposited) Token Account (ATA) address"
  fi

  INITIALIZE=$(
    yarn run ts-node scripts/initialize.ts \
    --vault "$VAULT_MINT" \
    --vault_token_account "$VAULT_TOKEN_ACCOUNT" \
    --mint "$MINT_TOKEN" \
    --unbonding_period "$UNBONDING_PERIOD" \
    --freeze_administrators "$FREEZE_ADMINISTRATORS" \
    --rewards_administrators "$REWARDS_ADMINISTRATORS")

  echo "$INITIALIZE"
  VAULT_AUTHORITY_PDA=$(echo $INITIALIZE | grep -oE 'Vault Authority PDA: ([A-Za-z0-9]+)' | awk '{print $NF}')
  CONFIG_PDA=$(echo $INITIALIZE | grep -oE 'Config PDA: ([A-Za-z0-9]+)' | awk '{print $NF}')
  MINT_AUTHORITY_PDA=$(echo $INITIALIZE | grep -oE 'Mint Authority PDA: ([A-Za-z0-9]+)' | awk '{print $NF}')
  FREEZE_AUTHORITY_PDA=$(echo $INITIALIZE | grep -oE 'Freeze Authority PDA: ([A-Za-z0-9]+)' | awk '{print $NF}')

  update_history_var "CONFIG_PDA"
  update_history_var "MINT_AUTHORITY_PDA"
  update_history_var "FREEZE_AUTHORITY_PDA"
  update_history_var "VAULT_AUTHORITY_PDA"
}

build_deploy_initialize() {
  build_program
  deploy_program
  initialize_program
}

update_solana_config() {
  solana config set --keypair "$KEYPAIR"
}

setup_metaplex() {
  if [ -z "$METAPLEX_NAME" ]; then
    prompt_with_default METAPLEX_NAME "Enter Metaplex Token Name"
  fi
  if [ -z "$METAPLEX_SYMBOL" ]; then
    prompt_with_default METAPLEX_SYMBOL "Enter Metaplex Token Symbol"
  fi
  if [ -z "$METAPLEX_META_URL" ]; then
    prompt_with_default METAPLEX_META_URL "Enter Metaplex Token Metadata URL (must be a valid JSON URL)"
  fi

  yarn run ts-node scripts/register_meta.ts \
    --mint "$MINT_TOKEN" \
    --keypair "$KEYPAIR" \
    --name "$METAPLEX_NAME" \
    --symbol "$METAPLEX_SYMBOL" \
    --token_meta_url "$METAPLEX_META_URL"
}

update_metaplex() {
  if [ -z "$METAPLEX_NAME" ]; then
    prompt_with_default METAPLEX_NAME "Enter Metaplex Token Name"
  fi
  if [ -z "$METAPLEX_SYMBOL" ]; then
    prompt_with_default METAPLEX_SYMBOL "Enter Metaplex Token Symbol"
  fi
  if [ -z "$METAPLEX_META_URL" ]; then
    prompt_with_default METAPLEX_META_URL "Enter Metaplex Token Metadata URL (must be a valid JSON URL)"
  fi

  yarn run ts-node scripts/register_meta.ts \
    --mint "$MINT_TOKEN" \
    --keypair "$KEYPAIR" \
    --name "$METAPLEX_NAME" \
    --symbol "$METAPLEX_SYMBOL" \
    --token_meta_url "$METAPLEX_META_URL" \
    --update
}

set_mint_and_freeze_authority() {
  echo "Setting Mint Authority to $MINT_AUTHORITY_PDA"
    spl-token authorize "$MINT_TOKEN" mint "$MINT_AUTHORITY_PDA" \
      --url "$SOLANA_URL" \
      --authority "$KEYPAIR"

  echo "Setting Freeze Authority to $FREEZE_AUTHORITY_PDA"
  spl-token authorize "$MINT_TOKEN" freeze "$FREEZE_AUTHORITY_PDA" \
    --url "$SOLANA_URL" \
    --authority "$KEYPAIR"
}

show_accounts_and_pdas() {
  echo ""
  echo "Program ID:                         $PROGRAM_ID"
  echo "Vault Token (accepted token):       $VAULT_MINT"
  echo "Mint Token (staking token minted):  $MINT_TOKEN"
  echo "Vault Token Authority Account:      $VAULT_TOKEN_ACCOUNT"
  echo "Config PDA:                         $CONFIG_PDA"
  echo "Mint Authority PDA:                 $MINT_AUTHORITY_PDA"
  echo "Freeze Authority PDA:               $FREEZE_AUTHORITY_PDA"
  echo "Vault Token Authority PDA:          $VAULT_AUTHORITY_PDA"
  echo "Freeze Administrators:              $FREEZE_ADMINISTRATORS"
  echo "Rewards Administrators:             $REWARDS_ADMINISTRATORS"
  echo "Unbonding Period (in seconds):      $UNBONDING_PERIOD"

  # get the mint token mint authority and freeze authority
  echo ""
  echo "Token Details:"
  spl-token display "$MINT_TOKEN" --url "$SOLANA_URL"
  echo ""
  spl-token display "$VAULT_MINT" --url "$SOLANA_URL"
  echo ""
  spl-token display "$VAULT_TOKEN_ACCOUNT" --url "$SOLANA_URL"
  echo ""
}

update_mint_authority() {
  prompt_with_default MINT_AUTHORITY "Enter new Mint Authority address"
  yarn run ts-node scripts/update_mint_authority.ts \
      --mint "$MINT_TOKEN" \
      --new_authority "$MINT_AUTHORITY"
}

update_freeze_authority() {
  prompt_with_default FREEZE_AUTHORITY "Enter new Freeze Authority address"
  yarn run ts-node scripts/update_freeze_authority.ts \
      --mint "$MINT_TOKEN" \
      --new_authority "$FREEZE_AUTHORITY"
}

update_unbonding_period() {
  prompt_with_default UNBONDING_PERIOD "Enter new Unbonding Period (in seconds)"
  yarn run ts-node scripts/update_config.ts \
      --unbonding_period "$UNBONDING_PERIOD"
}

while true; do
  MY_KEY=$(solana-keygen pubkey "$KEYPAIR")
  PROGRAM_ID=$(grep -oE 'declare_id!\("([A-Za-z0-9]+)"\);' ../programs/hastra-sol-vault-stake/src/lib.rs | grep -oE '\"([A-Za-z0-9]+)\"' | tr -d '"')

  echo ""

  SOL_BALANCE=$(solana balance --url "$SOLANA_URL" --keypair "$KEYPAIR" 2>/dev/null || echo "0 SOL")
  solana config get
  echo ""
  echo "Public Key: $MY_KEY ($SOL_BALANCE)"
  echo "Program ID: $PROGRAM_ID"
  echo ""

  echo "Select an action:"
  select opt in \
    "Build Program" \
    "Deploy Program" \
    "Initialize Program" \
    "Setup Metaplex" \
    "Set Mint and Freeze Authorities" \
    "Update Metaplex" \
    "Show Accounts & PDAs" \
    "Show Current Settings" \
    "Update Unbonding Period" \
    "Update Mint Authority" \
    "Update Freeze Authority" \
    "Create Mint Token" \
    "Create Vault Token Account" \
    "Reset and Set New Program ID" \
    "Exit"
  do
    case $REPLY in
      1) build_program; break ;;
      2) deploy_program; break ;;
      3) initialize_program; break ;;
      4) setup_metaplex; break ;;
      5) set_mint_and_freeze_authority; break ;;
      6) update_metaplex; break ;;
      7) show_accounts_and_pdas; break ;;
      8) show_current_settings; break ;;
      9) update_unbonding_period; break ;;
      10) update_mint_authority; break ;;
      11) update_freeze_authority; break ;;
      12) create_mint_token; break ;;
      13) create_vault_token_account; break ;;
      14) set_new_program_id; break ;;
      15) exit 0 ;;
      *) echo "Invalid option"; break ;;
    esac
  done
done
