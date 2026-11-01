## Shown at the top of the media check screen

# the number of files, and the total space used by files
# that have been moved to the trash folder. eg,
# "Trash folder: 3 files, 3.47MB"
media-check-trash-count =
    Trash folder: { $count ->
        [one] { $count } file, { $megs }MB
       *[other] { $count } files, { $megs }MB
    }
media-check-missing-count = Missing files: { $count }
media-check-unused-count = Unused files: { $count }
media-check-renamed-count = Renamed files: { $count }
media-check-oversize-count = Over 100MB: { $count }
media-check-subfolder-count = Subfolders: { $count }
media-check-extracted-count = Extracted images: { $count }

## Shown at the top of each section

media-check-renamed-header = Some files have been renamed for compatibility:
media-check-oversize-header = Files over 100MB can not be synced with AnkiWeb.
media-check-subfolder-header = Folders inside the media folder are not supported.
media-check-missing-header = The following files are referenced by cards, but were not found in the media folder:
media-check-unused-header = The following files were found in the media folder, but do not appear to be used on any cards:
media-check-template-references-field-header =
    Anki can not detect used files when you use { "{{Field}}" } references in media/LaTeX tags. The media/LaTeX tags should be placed on individual notes instead.
    
    Referencing templates:

## Shown once for each file

media-check-renamed-file = Renamed: { $old } -> { $new }
media-check-oversize-file = Over 100MB: { $filename }
media-check-subfolder-file = Folder: { $filename }
media-check-missing-file = Missing: { $filename }
media-check-unused-file = Unused: { $filename }

##

# Eg "Basic: Card 1 (Front Template)"
media-check-notetype-template = { $notetype }: { $card_type } ({ $side })

## Progress

media-check-checked = Checked { $count }...

## Deleting unused media


## Rendering LaTeX


## Buttons

