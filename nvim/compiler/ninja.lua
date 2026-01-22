local function setup()
  vim.b.current_compiler = "ninja"

  vim.opt_local.errorformat = table.concat({
    [[%Dninja: Entering directory `%f']],
    [[%Xninja: Leaving directory `%f']],

    -- clang/gcc
    [[%f:%l:%c: %t%*[^:]: %m]],
    [[%f:%l: %t%*[^:]: %m]],

    -- lld
    [[%Eld.lld: error: undefined symbol: %m]],
    [[%C>>> referenced by %*[^ ] (%f:%l)]],
    [[%C>>> %m]],

    -- ignore unmatched lines
    [[%-G%.%#]],
  }, ",")
  vim.opt_local.makeprg = "ninja"
end

setup()
