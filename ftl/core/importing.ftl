importing-cannot-merge-notetypes-of-different-kinds =
    Cloze note types cannot be merged with regular note types.
    You may still import the file with '{ importing-merge-notetypes }' disabled.
importing-file-must-contain-field-column = File must contain at least one column that can be mapped to a note field.
# the action of combining two existing note types to create a new one
importing-merge-notetypes = Merge note types
importing-the-provided-file-is-not-a = The provided file is not a valid .apkg file.
importing-processed-media-file =
    { $count ->
        [one] Imported { $count } media file
       *[other] Imported { $count } media files
    }
importing-importing-file = Importing file...
importing-extracting = Extracting data...
importing-gathering = Gathering data...
importing-failed-to-import-media-file = Failed to import media file: { $debugInfo }
importing-processed-notes =
    { $count ->
        [one] Processed { $count } note...
       *[other] Processed { $count } notes...
    }
importing-processed-cards =
    { $count ->
        [one] Processed { $count } card...
       *[other] Processed { $count } cards...
    }
importing-file-empty = The file you selected is empty.

## NO NEED TO TRANSLATE. This text is no longer used by Anki, and will be removed in the future.

