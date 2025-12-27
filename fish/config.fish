function is_mac
  return (test (uname) = Darwin)
end

function is_linux
  return (test (uname) = Linux)
end

function llvmenv
  if is_mac
    set -x LDFLAGS "-L/opt/homebrew/opt/llvm/lib/c++,/opt/homebrew/opt/llvm/lib -Wl,-rpath,/opt/homebrew/opt/llvm/lib/c++"
    set -x CPPFLAGS "-I/opt/homebrew/opt/llvm/include"
    add_path "/opt/homebrew/opt/llvm/bin"
  else
    # linux: nothing to do
  end
end

function add_path
  if test (count $argv)
    for path in $argv
      set expandeds $path
      for expanded in $expandeds
        fish_add_path $expanded -p
      end
    end
  end
end

function vim
  if test -x (which nvim)
    command nvim $argv
  else
    command vim $argv
  end
end

set fish_greeting

# 放到最前面，后面有些命令会依赖
if is_mac
  eval (/opt/homebrew/bin/brew shellenv)
  add_path '/opt/homebrew/bin'
  add_path '/opt/homebrew/sbin'
end

## aliases
alias ls 'ls --color'
alias cls 'clear && echo -ne "\e[3J"'
alias jqless 'jq --color-output | less -r'
alias less 'less --mouse --wheel-lines=3'
alias ffmpeg 'ffmpeg -hide_banner'
alias fd 'fd --no-ignore --hidden'
alias pd prevd
alias nd nextd
alias cdr 'cd (git root)'
alias claude 'ccr code'

## binds
bind alt-backspace backward-kill-path-component

## envs
set -gx EDITOR nvim
set -gx ANDROID_HOME (if is_mac; echo ~/Library/Android/sdk; else; echo /opt/android-sdk; end)
set -gx ANDROID_SDK_ROOT (if is_mac; echo ~/Library/Android/sdk; else; echo /opt/android-sdk; end)
set -gx PAGER 'less --mouse --wheel-lines=3'
set -gx HOMEBREW_NO_AUTO_UPDATE 1

set -gx tide_git_truncation_length 40

add_path '~/.local/opt/*/bin' ~/.local/bin ~/scripts ~/script /usr/local/bin ~/bin
add_path ~/src/emsdk/ ~/src/emsdk/upstream/emscripten/
add_path ~/.ghcup/bin
add_path $ANDROID_SDK_ROOT/platform-tools
add_path ~/.dotnet/tools/
add_path ~/src/blade-build/
if is_mac
  add_path /Applications/Xcode.app/Contents/Developer/usr/bin
  add_path /opt/homebrew/opt/coreutils/libexec/gnubin
  add_path ~/.lmstudio/bin
end

# pyenv
if type pyenv -q && test -z "$PYENV_SHELL"
  pyenv init --no-rehash - | source
end

# if test "$JENV_LOADED" != "1"
#   status --is-interactive; 
#     and jenv init - | source && jenv enable-plugin export >/dev/null
#   # status --is-interactive; and jenv enable-plugin export
# end
set -e JAVA_HOME

# Wasmer
if test -d ~/.wasmer
  set -gx WASMER_DIR ~/.wasmer
  set -gx WASMER_CACHE_DIR "$WASMER_DIR/cache"
  add_path "$WASMER_DIR/bin" "$WASMER_DIR/globals/wapm_packages/.bin"
end

if type fnm -q && status is-interactive
  fnm env --use-on-cd --version-file-strategy recursive --shell fish --log-level quiet | .
  fnm completions --shell fish | .
  alias nvm=fnm
end

if test -f ~/.bashrc
  apply_env ~/.bashrc
end

if type rbenv -q && status --is-interactive
  rbenv init - --no-rehash fish | source
end
