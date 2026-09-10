# Provider connections

Use `/login` and `/logout` from the prompt to manage provider credentials without leaving your deck or conversation. Both commands are also in `/actions`.

## Connect a provider

1. Run `/login`.
2. Choose a provider and, if offered, an authentication method.
3. Complete browser or device sign-in, or enter an API key.
4. Run `/model` to select a model from your connected providers.

Login does not change your selected model. You can reconnect an existing provider to replace its saved credentials. API keys are masked during entry and saved in slide-builder's OS keyring, separate from Rho's credentials.

Escape returns to the previous step. Escape from the provider list returns to the workspace.

## Remove a connection

Run `/logout`, choose a saved connection, and confirm removal. For providers with multiple authentication methods, each saved method is a separate connection.

Logout removes the local credential, not the provider account, and does not revoke tokens on the provider's website. Environment variables and credentials managed by external tools are not deleted.

The model list refreshes after connection changes. If you remove the current model's credentials and no external credential remains, new prompts and design imports are blocked until you reconnect with `/login` or choose another connected model with `/model`. Your deck and conversation remain open.

Finish or cancel an active agent run or design import before managing connections.
