// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use camino::Utf8Path;

use crate::action::BuildAction;
use crate::input::BuildInput;

/// Copy the provided files into the specified destination folder.
/// Directory structure is not preserved - eg foo/bar.js is copied
/// into out/$output_folder/bar.js.
pub struct CopyFiles<'a> {
    pub inputs: BuildInput,
    /// The folder (relative to the build folder) that files should be copied
    /// into.
    pub output_folder: &'a str,
}

impl BuildAction for CopyFiles<'_> {
    fn command(&self) -> &str {
        // The -f is because we may need to overwrite read-only files copied from Bazel.
        "cp -fr $in $builddir/$folder"
    }

    fn files(&mut self, build: &mut impl crate::build::FilesHandle) {
        let inputs = build.expand_inputs(&self.inputs);
        let output_folder = Utf8Path::new(self.output_folder);
        let outputs: Vec<_> = inputs
            .iter()
            .map(|f| output_folder.join(Utf8Path::new(f).file_name().unwrap()))
            .collect();
        build.add_inputs("in", &self.inputs);
        build.add_outputs("", outputs);
        build.add_variable("folder", self.output_folder);
    }
}
