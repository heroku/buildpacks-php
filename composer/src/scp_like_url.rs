use std::fmt;
use url::Host;

/// An SCP-like Git remote such as `git@github.com:foo/bar.git`, in the form `[user@]host:path`.
///
/// The path is relative to the remote host and is not a local filesystem path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScpLikeUrl {
    user: Option<String>,
    host: Host<String>,
    path: String,
}

impl ScpLikeUrl {
    /// Parses `[user@]host:path`, returning `None` for anything else.
    ///
    /// Windows drive paths (such as `user@C:\foo`) are rejected.
    #[must_use]
    pub fn parse(input: &str) -> Option<Self> {
        let (user, rest) = match input.split_once('@') {
            Some((user, rest)) => (Some(user), rest),
            None => (None, input),
        };

        if user.is_some_and(|user| user.is_empty() || !user.chars().all(is_name_char)) {
            return None;
        }

        let (host, path) = rest.split_once(':')?;

        if !host.starts_with(char::is_alphanumeric) || !host.chars().all(is_name_char) {
            return None;
        }

        if path.is_empty()
            || path.starts_with('\\')
            || path.starts_with("//")
            || path.contains('\0')
        {
            return None;
        }

        Some(Self {
            user: user.map(str::to_string),
            host: Host::parse_opaque(host).ok()?,
            path: path.to_string(),
        })
    }

    #[must_use]
    pub fn user(&self) -> Option<&str> {
        self.user.as_deref()
    }

    #[must_use]
    pub fn host(&self) -> &Host<String> {
        &self.host
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl fmt::Display for ScpLikeUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(user) = &self.user {
            write!(f, "{user}@")?;
        }
        write!(f, "{}:{}", self.host, self.path)
    }
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric()
        || matches!(
            c,
            '-' | '.'
                | '_'
                | '~'
                | '!'
                | '$'
                | '&'
                | '\''
                | '('
                | ')'
                | '*'
                | '+'
                | ','
                | ';'
                | '='
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_user_host_and_path() {
        let url = ScpLikeUrl::parse("git@github.com:foo/bar.git").unwrap();
        assert_eq!(url.user(), Some("git"));
        assert_eq!(url.host().to_string(), "github.com");
        assert_eq!(url.path(), "foo/bar.git");
    }

    #[test]
    fn parses_without_user() {
        let url = ScpLikeUrl::parse("my_host:foo/bar").unwrap();
        assert_eq!(url.user(), None);
        assert_eq!(url.host().to_string(), "my_host");
        assert_eq!(url.path(), "foo/bar");
    }

    #[test]
    fn path_with_and_without_dot_git() {
        assert_eq!(
            ScpLikeUrl::parse("git@h:foo/bar.git").unwrap().path(),
            "foo/bar.git"
        );
        assert_eq!(
            ScpLikeUrl::parse("git@h:foo/bar").unwrap().path(),
            "foo/bar"
        );
    }

    #[test]
    fn path_without_slash() {
        assert_eq!(ScpLikeUrl::parse("git@h:repo").unwrap().path(), "repo");
    }

    #[test]
    fn absolute_and_home_relative_paths() {
        assert_eq!(
            ScpLikeUrl::parse("git@h:/srv/foo.git").unwrap().path(),
            "/srv/foo.git"
        );
        assert_eq!(
            ScpLikeUrl::parse("git@h:~/foo.git").unwrap().path(),
            "~/foo.git"
        );
    }

    #[test]
    fn display_round_trips() {
        for case in [
            "git@github.com:foo/bar.git",
            "git@server.com:~/foo/bar.git",
            "my_host:foo/bar",
            "git@h:/srv/foo.git",
        ] {
            assert_eq!(ScpLikeUrl::parse(case).unwrap().to_string(), case);
        }
    }

    #[test]
    fn rejects_windows_paths() {
        assert_eq!(ScpLikeUrl::parse("git@C:\\foo"), None);
        assert_eq!(ScpLikeUrl::parse("C:\\foo"), None);
    }

    #[test]
    fn rejects_inputs_with_a_scheme() {
        for case in [
            "https://github.com/foo/bar.git",
            "ssh://git@github.com/foo/bar.git",
            "ssh://git@github.com:22/foo/bar.git",
            "file:///foo/bar",
        ] {
            assert_eq!(ScpLikeUrl::parse(case), None, "{case}");
        }
    }

    #[test]
    fn rejects_empty_host_or_path() {
        for case in [
            "git@:foo/bar",
            ":foo/bar",
            "git@host:",
            "host:",
            "git@",
            "@host:foo",
            ":",
            "",
        ] {
            assert_eq!(ScpLikeUrl::parse(case), None, "{case}");
        }
    }

    #[test]
    fn rejects_local_paths() {
        for case in ["/foo/bar", "../foo/bar", "foo/bar", "./foo@bar:baz", "foo"] {
            assert_eq!(ScpLikeUrl::parse(case), None, "{case}");
        }
    }

    #[test]
    fn parses_numeric_first_path_segment() {
        for case in ["git@host:22/repository.git", "git@host:2222"] {
            assert_eq!(ScpLikeUrl::parse(case).unwrap().to_string(), case);
        }
    }

    #[test]
    fn rejects_credentials_with_password_or_extra_at_sign() {
        for case in ["user:pw@host:foo/bar", "a@b@host:foo"] {
            assert_eq!(ScpLikeUrl::parse(case), None, "{case}");
        }
    }
}
