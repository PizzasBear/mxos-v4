local lspc = require "lspconfig"
lspc.rust_analyzer.setup {
    settings = {
        ["rust-analyzer"] = {
            checkOnSave = {
                overrideCommand = { "cargo", "check", "--message-format=json" },
            },
            -- cargo = {
            --     target = "x86_64-unknown-uefi",
            --     arguments = { "--target", "x86_64-unknown-uefi" },
            -- },
        },
    },
}
