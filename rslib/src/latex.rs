// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

use crate::cloze::expand_clozes_to_reveal_latex;
use crate::media::files::sha1_of_data;
use crate::text::strip_html;

pub(crate) static LATEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?xsi)
            \[latex\](.+?)\[/latex\]     # 1 - standard latex
            |
            \[\$\](.+?)\[/\$\]           # 2 - inline math
            |
            \[\$\$\](.+?)\[/\$\$\]       # 3 - math environment
            ",
    )
    .unwrap()
});
static LATEX_NEWLINES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?xi)
            <br( /)?>
            |
            <div>
        "#,
    )
    .unwrap()
});

pub(crate) fn contains_latex(text: &str) -> bool {
    LATEX.is_match(text)
}

/// The media filenames that the LaTeX in this text would have been rendered
/// to, after expanding any cloze deletions. EvolAnki doesn't generate LaTeX,
/// but media check keeps existing images so they aren't reported as unused.
pub(crate) fn latex_filenames_expanding_clozes(text: &str, svg: bool) -> Vec<String> {
    if text.contains("{{c") {
        latex_filenames(&expand_clozes_to_reveal_latex(text), svg)
    } else {
        latex_filenames(text, svg)
    }
}

/// The media filenames that the LaTeX in this text would have been rendered
/// to. Expects cloze deletions to already be expanded.
pub(crate) fn latex_filenames(text: &str, svg: bool) -> Vec<String> {
    LATEX
        .captures_iter(text)
        .map(|caps| {
            let latex = match (caps.get(1), caps.get(2), caps.get(3)) {
                (Some(m), _, _) => m.as_str().into(),
                (_, Some(m), _) => format!("${}$", m.as_str()),
                (_, _, Some(m)) => {
                    format!(r"\begin{{displaymath}}{}\end{{displaymath}}", m.as_str())
                }
                _ => unreachable!(),
            };
            fname_for_latex(&strip_html_for_latex(&latex), svg)
        })
        .collect()
}

fn strip_html_for_latex(html: &str) -> Cow<'_, str> {
    let mut out: Cow<str> = html.into();
    if let Cow::Owned(o) = LATEX_NEWLINES.replace_all(html, "\n") {
        out = o.into();
    }
    if let Cow::Owned(o) = strip_html(out.as_ref()) {
        out = o.into();
    }

    out
}

fn fname_for_latex(latex: &str, svg: bool) -> String {
    let ext = if svg { "svg" } else { "png" };
    let csum = hex::encode(sha1_of_data(latex.as_bytes()));

    format!("latex-{csum}.{ext}")
}

#[cfg(test)]
mod test {
    use super::latex_filenames;

    #[test]
    fn latex() {
        assert_eq!(
            latex_filenames("a[latex]one<br>and<div>two[/latex]b", false),
            vec!["latex-ef30b3f4141c33a5bf7044b0d1961d3399c05d50.png"]
        );
        assert_eq!(
            latex_filenames("[$]<b>hello</b>&nbsp; world[/$]", true),
            vec!["latex-060219fbf3ddb74306abddaf4504276ad793b029.svg"]
        );
        assert_eq!(
            latex_filenames("[$$]math &amp; stuff[/$$]", false),
            vec!["latex-8899f3f849ffdef6e4e9f2f34a923a1f608ebc07.png"]
        );
    }
}
