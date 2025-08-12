#!/usr/bin/env bash
set -eu
set -o pipefail

our_name=$(basename "$0")

show_welcome=
show_usage=
process_type=
if (( $# == 0 )); then
	# no args at all, main usage screen
	show_usage=1
	show_welcome=1
elif [[ "$1" == "help" ]] || [[ "$1" == "--help" ]]; then
	# main usage screen unless more args come later
	show_usage=1
	shift # drop arg
fi
if [[ -n "$show_usage" ]] && (( $# > 0 )); then
	# show specific process type help instead of main usage screen
	show_usage=
	process_type=$1
	shift # drop arg
fi

# readarray -d '' uses zero-bytes as the delimiter
# find all executable files (regular or link) in /cnb/process/ (except ourselves)
readarray -d '' process_types < <(find /cnb/process/* -maxdepth 0 -executable -type f,l -not -name "$our_name" -printf '%f\0')

if [[ -n "$show_usage" ]]; then
	if [[ -n "$show_welcome" ]]; then
		cat >&2 <<-'EOF'
			
			██╗    ██╗███████╗██╗      ██████╗ ██████╗ ██╗   ██╗███████╗
			██║    ██║██╔════╝██║     ██╔════╝██╔═══██╗███╗ ███║██╔════╝
			██║ █╗ ██║█████╗  ██║     ██║     ██║   ██║█╔████╔█║█████╗  
			██║███╗██║██╔══╝  ██║     ██║     ██║   ██║█║╚██╔╝█║██╔══╝  
			╚███╔███╔╝███████╗███████╗╚██████╗╚██████╔╝█║ ╚═╝ █║███████╗
			 ╚══╝╚══╝ ╚══════╝╚══════╝ ╚═════╝ ╚═════╝ ═╝     ╚╝╚══════╝
			
			This help screen is the default process type for your CNB app image.
			
			It can provide general instructions, list available process types, show help
			for specific process types, and execute arbitrary commands.
			
			Invoking this Usage Help
			========================
			
			Running your image without any arguments, or with `help' or `--help',
			will display this screen:
			
			    $ docker run --rm <this-image>
			    $ docker run --rm <this-image> help
			    $ docker run --rm <this-image> --help
		EOF
	else
		cat >&2 <<-'EOF'
			
			██╗   ██╗ ███████╗ █████╗   █████╗ ███████╗
			██║   ██║ ██╔════╝██╔══██╗ ██╔═══╝ ██╔════╝
			██║   ██║ ███████╗███████║ ██║ ███╗█████╗  
			██║   ██║ ╚════██║██╔══██║ ██║  ██║██╔══╝  
			╚██████╔╝ ███████║██║  ██║ ╚█████╔╝███████╗
			 ╚═════╝  ╚══════╝╚═╝  ╚═╝  ╚════╝ ╚══════╝
		EOF
	fi
	
	cat >&2 <<-'EOF'
		
		Basic Usage Summary
		===================
		
		    $ docker run --rm <image-name> (help | --help) [process-type]
		    $ docker run --rm --entrypoint <process-type> <this-image> [<argument>...]
		    $ docker run --rm [-it] <this-image> [--] <command> [<argument>...]
		
		Available Process Types
		=======================
		
		The following process types are available in this image:
		
	EOF
	printf '  - %s\n' "${process_types[@]}" >&2
	cat >&2 <<-'EOF'
		
		Getting Help for a Process Type
		===============================
		
		To show help for a process type, pass its name after `help', like so:
		
		    $ docker run --rm <this-image> help <process-type>
		
		Launching a Process Type
		========================
		
		To launch a specific process type, specify it as the `--entrypoint', e.g.:
		
		    $ docker run --rm --entrypoint <process-type> <this-image>
		
		Some process types may require certain environment variables to be set, or
		ports to be forwarded from the container, in order to be usable. Refer to
		the help output for the respective process type for further information.
		
		For example, a `web' process type typically requires a forwarded port, and
		the environment variable `$PORT' specifying the in-container port number:
		
		    $ docker run --rm --entrypoint web -p 8080:8080 -e PORT=8080 <this-image>
		
		Executing commands
		==================
		
		When no entrypoint is specified (or with `--entrypoint usage'), and when
		not providing `help' or `--help' as the first argument after the image name,
		the given arguments will be executed as regular commands.
		
		To launch an interactive shell, use the `-it' option, and specify `bash' as
		the command:
		
		    $ docker run --rm -it <this-image> bash
		
		You may pass arbitrary additional arguments to commands, for example:
		
		    $ docker run --rm -it <this-image> bash --login
		
		To completely bypass this help tool, specify `--entrypoint launcher', e.g.:
		
		    $ docker run --rm -it --entrypoint launcher <this-image> uname -a
		
		Further reading
		===============
		
		For additional documentation on how to run buildpacks-built images, refer to
		the documentation at Buildpacks.io:
		  https://buildpacks.io/docs/for-app-developers/how-to/build-outputs/specify-launch-process/
		
	EOF
	exit 2
fi

if [[ -n "$process_type" ]]; then
	if ! printf '%s\0' "${process_types[@]}" | grep -Fqxz -- "$process_type"; then
		echo "Unknown process type: $process_type" >&2
		exit 1
	fi
	
	# crudely (it's a PoC) find the buildpack ID for this process type
	buildpack_id=$(
		sed -n '/^\[\[processes\]\]$/,$p' /layers/config/metadata.toml | # from first "[[processes]]" entry
		sed -n -E '/^\s*type\s*=\s*\"'"$process_type"'\"/,/\[\[/p' | # type field equals our process type, get until next "[["
		sed -n -E 's/^\s*buildpack-id\s*=\s*\"([^\"]+)\"/\1/p' | # get the buildpack-id field value
		tr "/" "_" # translate "/" to "_"
	)
	
	helpfile="/layers/${buildpack_id}/usage_help/${process_type}.txt"
	
	if [[ -f "$helpfile" ]]; then
		# output with prefix
		cat "$helpfile" >&2
	else
		# the printf "0*d" command outputs a zero "*" times, and we give the length of $process_type as the count
		cat >&2 <<-EOF
			
			Usage: \`${process_type}'
			=========$(printf "%0*d" ${#process_type} | tr "0" "=")
			
			Launching this Process Type
			===========================
			
			To launch this process type, specify it as the \`--entrypoint':
			
			    $ docker run --rm --entrypoint ${process_type} <image-name>
			
		EOF
	fi
	
	exit 0
fi

if [[ "$1" == "--" ]]; then
	shift
fi

exec "$@"
