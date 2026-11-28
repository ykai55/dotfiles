function open --description "Open files in their default applications, using Bambu Studio for .3mf"
  argparse -n open h/help -- $argv
  or return

  if set -q _flag_help
    __fish_print_help open
    return 0
  end

  if not set -q argv[1]
    printf (_ "%s: Expected at least %d args, got only %d\n") open 1 0 >&2
    return 1
  end

  # macOS ships its own open(1); defer to it.
  if test -x /usr/bin/open
    command open $argv
    return
  end

  if type -q -f cygstart
    for i in $argv
      cygstart $i
    end
  else if type -q -f xdg-open
    for i in $argv
      # .3mf is a zip container, so xdg-mime reports application/zip or
      # application/octet-stream and xdg-open falls back to the browser.
      # Bambu Studio chdirs before loading its arguments, so hand it an
      # absolute path or a relative one stops resolving.
      if string match -qi '*.3mf' -- $i; and type -q -f bambu-studio
        command bambu-studio (path resolve -- "$i") >/dev/null 2>&1 &
        disown $last_pid
      else
        xdg-open $i
      end
    end
  else
    echo (_ 'No open utility found. Try installing "xdg-open" or "xdg-utils".') >&2
  end
end