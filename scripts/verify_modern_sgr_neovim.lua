-- T0003 native Neovim fixture: nvim --clean -n -i NONE -S scripts/verify_modern_sgr_neovim.lua
-- Explicit application highlight setup; no forced terminal identity/capability options.
vim.opt.termguicolors = true
vim.opt.swapfile = false
vim.opt.shadafile = 'NONE'
vim.opt.number = true
vim.opt.showmode = true
vim.opt.laststatus = 2
vim.opt.statusline = 'T0003 modern SGR | edit + Ctrl-L | resize | :qa!'
vim.opt.background = 'dark'
vim.api.nvim_set_hl(0, 'Normal', { fg = '#dddddd', bg = '#202020' })
vim.api.nvim_set_hl(0, 'HarborCurlRed', { undercurl = true, sp = '#ff5050' })
vim.api.nvim_set_hl(0, 'HarborCurlGreen', { undercurl = true, sp = '#50ff50' })
vim.api.nvim_set_hl(0, 'HarborDouble', { underdouble = true, sp = '#5080ff' })
vim.api.nvim_set_hl(0, 'HarborDotted', { underdotted = true, sp = '#ffff50' })
vim.api.nvim_set_hl(0, 'HarborDashed', { underdashed = true, sp = '#ff50ff' })
vim.api.nvim_set_hl(0, 'DiagnosticUnderlineError', { undercurl = true, sp = '#ff5050' })
vim.api.nvim_buf_set_lines(0, 0, -1, false, {
  'T0003 Neovim controlled modern SGR acceptance',
  'RED UNDERCURL: diagnostic error    wide: 界    spaces',
  'GREEN UNDERCURL: independent special color    spaces',
  'BLUE DOUBLE: double underline    wide: 界    spaces',
  'YELLOW DOTTED: dotted underline    wide: 界    spaces',
  'MAGENTA DASHED: dashed underline    wide: 界    spaces',
  'Edit this line, leave insert mode, Ctrl-L; resize narrow/wide.',
  'Exit with :qa!; subsequent shell text must be ordinary.',
})
local ns = vim.api.nvim_create_namespace('harbor-modern-sgr')
for index, name in ipairs({ 'HarborCurlRed', 'HarborCurlGreen', 'HarborDouble', 'HarborDotted', 'HarborDashed' }) do
  vim.api.nvim_buf_set_extmark(0, ns, index, 0, {
    end_row = index, end_col = #vim.api.nvim_buf_get_lines(0, index, index + 1, false)[1],
    hl_group = name,
  })
end
vim.diagnostic.config({ virtual_text = false, signs = false, underline = true })
vim.diagnostic.set(vim.api.nvim_create_namespace('harbor-diagnostic'), 0, {
  { lnum = 1, col = 15, end_col = 31, message = 'Controlled fixture error', severity = vim.diagnostic.severity.ERROR },
})
vim.api.nvim_win_set_cursor(0, { 7, 0 })
