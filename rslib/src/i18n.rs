// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! The text shown to users. EvolAnki is English-only, so these are plain
//! strings, reached with `tr.some_string(args)`.

use std::borrow::Cow;
use std::fmt;
use std::fmt::Display;

/// Gives out the user-visible strings. It holds no data; it exists so that
/// code can keep calling `tr.some_string()`.
#[derive(Clone, Debug, Default)]
pub struct I18n;

impl I18n {
    pub fn new() -> Self {
        Self
    }
}

/// A number that can be shown in a string.
pub trait Number {
    fn to_f64(self) -> f64;
}

macro_rules! impl_number {
    ($($t:ty)+) => {
        $(impl Number for $t {
            fn to_f64(self) -> f64 {
                self as f64
            }
        })+
    };
}

impl_number!(i32 i64 u16 u32 u64 usize);

impl Number for f32 {
    /// Rounded to two decimal places first, so that 1.001 reads "1 card".
    fn to_f64(self) -> f64 {
        ((self * 100.0).round() / 100.0) as f64
    }
}

/// A number as it is shown: at most two decimal places, without digit
/// grouping.
struct Num(f64);

impl Num {
    fn new(n: impl Number) -> Self {
        Self(n.to_f64())
    }

    /// Whether English uses the singular, as in "1 card".
    fn is_one(&self) -> bool {
        self.0.abs() == 1.0
    }
}

impl Display for Num {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = format!("{:.2}", self.0);
        f.write_str(text.trim_end_matches('0').trim_end_matches('.'))
    }
}

impl I18n {
    pub fn actions_custom_study(&self) -> Cow<'static, str> {
        "Custom Study".into()
    }

    pub fn actions_empty_cards(&self) -> Cow<'static, str> {
        "Empty Cards".into()
    }

    pub fn actions_import(&self) -> Cow<'static, str> {
        "Import".into()
    }

    pub fn actions_rename_deck(&self) -> Cow<'static, str> {
        "Rename Deck".into()
    }

    pub fn actions_rename_tag(&self) -> Cow<'static, str> {
        "Rename Tag".into()
    }

    pub fn actions_remove_tag(&self) -> Cow<'static, str> {
        "Remove Tag".into()
    }

    pub fn actions_reposition(&self) -> Cow<'static, str> {
        "Reposition".into()
    }

    pub fn actions_set_due_date(&self) -> Cow<'static, str> {
        "Set Due Date".into()
    }

    pub fn actions_toggle_load_balancer(&self) -> Cow<'static, str> {
        "Toggle Load Balancer".into()
    }

    pub fn actions_grade_now(&self) -> Cow<'static, str> {
        "Grade Now".into()
    }

    pub fn actions_answer_card(&self) -> Cow<'static, str> {
        "Answer Card".into()
    }

    pub fn actions_unsuspend(&self) -> Cow<'static, str> {
        "Unsuspend".into()
    }

    pub fn actions_add_deck(&self) -> Cow<'static, str> {
        "Add Deck".into()
    }

    pub fn actions_add_note(&self) -> Cow<'static, str> {
        "Add Note".into()
    }

    pub fn actions_update_tag(&self) -> Cow<'static, str> {
        "Update Tag".into()
    }

    pub fn actions_update_note(&self) -> Cow<'static, str> {
        "Update Note".into()
    }

    pub fn actions_update_deck(&self) -> Cow<'static, str> {
        "Update Deck".into()
    }

    pub fn actions_forget_card(&self) -> Cow<'static, str> {
        "Reset Card".into()
    }

    pub fn actions_build_filtered_deck(&self) -> Cow<'static, str> {
        "Build Deck".into()
    }

    pub fn actions_add_notetype(&self) -> Cow<'static, str> {
        "Add Note Type".into()
    }

    pub fn actions_update_notetype(&self) -> Cow<'static, str> {
        "Update Note Type".into()
    }

    pub fn actions_update_config(&self) -> Cow<'static, str> {
        "Update Config".into()
    }

    pub fn browsing_cards_cant_be_manually_moved_into(&self) -> Cow<'static, str> {
        "Cards can't be manually moved into a filtered deck.".into()
    }

    pub fn browsing_change_deck(&self) -> Cow<'static, str> {
        "Change Deck".into()
    }

    pub fn browsing_select_deck(&self) -> Cow<'static, str> {
        "Select Deck".into()
    }

    pub fn browsing_row_deleted(&self) -> Cow<'static, str> {
        "(deleted)".into()
    }

    pub fn card_template_rendering_more_info(&self) -> Cow<'static, str> {
        "More information".into()
    }

    pub fn card_template_rendering_front_side_problem(&self) -> Cow<'static, str> {
        "Front template has a problem:".into()
    }

    pub fn card_template_rendering_back_side_problem(&self) -> Cow<'static, str> {
        "Back template has a problem:".into()
    }

    pub fn card_template_rendering_browser_front_side_problem(&self) -> Cow<'static, str> {
        "Browser-specific front template has a problem:".into()
    }

    pub fn card_template_rendering_browser_back_side_problem(&self) -> Cow<'static, str> {
        "Browser-specific back template has a problem:".into()
    }

    pub fn card_template_rendering_conditional_not_closed(
        &self,
        missing: impl Display,
    ) -> Cow<'static, str> {
        format!("Missing '{missing}'").into()
    }

    pub fn card_template_rendering_wrong_conditional_closed(
        &self,
        found: impl Display,
        expected: impl Display,
    ) -> Cow<'static, str> {
        format!("Found '{found}', but expected '{expected}'").into()
    }

    pub fn card_template_rendering_conditional_not_open(
        &self,
        found: impl Display,
        missing1: impl Display,
        missing2: impl Display,
    ) -> Cow<'static, str> {
        format!("Found '{found}', but missing '{missing1}' or '{missing2}'").into()
    }

    pub fn card_template_rendering_no_such_field(
        &self,
        found: impl Display,
        field: impl Display,
    ) -> Cow<'static, str> {
        format!("Found '{found}', but there is no field called '{field}'").into()
    }

    pub fn card_template_rendering_empty_front(&self) -> Cow<'static, str> {
        "The front of this card is blank.".into()
    }

    pub fn card_template_rendering_missing_cloze(&self, number: impl Number) -> Cow<'static, str> {
        let number = Num::new(number);
        format!("No cloze {number} found on card.\nPlease either add a cloze deletion, or use the Empty Cards tool.").into()
    }

    pub fn card_templates_front_template(&self) -> Cow<'static, str> {
        "Front Template".into()
    }

    pub fn card_templates_back_template(&self) -> Cow<'static, str> {
        "Back Template".into()
    }

    pub fn card_templates_invalid_template_number(
        &self,
        number: impl Number,
        notetype: impl Display,
    ) -> Cow<'static, str> {
        let number = Num::new(number);
        format!("Card template {number} in note type '{notetype}' has a problem.").into()
    }

    pub fn card_templates_identical_front(&self, number: impl Number) -> Cow<'static, str> {
        let number = Num::new(number);
        format!("The front side is identical to card template {number}.").into()
    }

    pub fn card_templates_no_front_field(&self) -> Cow<'static, str> {
        "Expected to find a field replacement on the front of the card template.".into()
    }

    pub fn card_templates_missing_cloze(&self) -> Cow<'static, str> {
        "Expected to find '{{cloze:Text}}' or similar on the front and back of the card template."
            .into()
    }

    pub fn card_templates_see_preview(&self) -> Cow<'static, str> {
        "See the preview for more information.".into()
    }

    pub fn card_templates_field_not_found(&self, field: impl Display) -> Cow<'static, str> {
        format!("Field '{field}' not found.").into()
    }

    pub fn custom_study_no_cards_matched_the_criteria_you(&self) -> Cow<'static, str> {
        "No cards matched the criteria you provided.".into()
    }

    pub fn custom_study_must_rename_deck(&self) -> Cow<'static, str> {
        "Please rename the existing Custom Study deck first.".into()
    }

    pub fn custom_study_custom_study_session(&self) -> Cow<'static, str> {
        "Custom Study Session".into()
    }

    pub fn database_check_corrupt(&self) -> Cow<'static, str> {
        "Collection file is corrupt. Please restore from an automatic backup.".into()
    }

    pub fn database_check_card_properties(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        if count.is_one() {
            format!("Fixed {count} invalid card property.").into()
        } else {
            format!("Fixed {count} invalid card properties.").into()
        }
    }

    pub fn database_check_card_last_review_time_empty(
        &self,
        count: impl Number,
    ) -> Cow<'static, str> {
        let count = Num::new(count);
        if count.is_one() {
            format!("Added last review time to {count} card.").into()
        } else {
            format!("Added last review time to {count} cards.").into()
        }
    }

    pub fn database_check_missing_templates(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        if count.is_one() {
            format!("Deleted {count} card with missing template.").into()
        } else {
            format!("Deleted {count} cards with missing template.").into()
        }
    }

    pub fn database_check_field_count(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        if count.is_one() {
            format!("Fixed {count} note with wrong field count.").into()
        } else {
            format!("Fixed {count} notes with wrong field count.").into()
        }
    }

    pub fn database_check_new_card_high_due(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        if count.is_one() {
            format!("Found {count} new card with a due number >= 1,000,000 - consider repositioning it in the Browse screen.").into()
        } else {
            format!("Found {count} new cards with a due number >= 1,000,000 - consider repositioning them in the Browse screen.").into()
        }
    }

    pub fn database_check_card_missing_note(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        if count.is_one() {
            format!("Deleted {count} card with missing note.").into()
        } else {
            format!("Deleted {count} cards with missing note.").into()
        }
    }

    pub fn database_check_duplicate_card_ords(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        if count.is_one() {
            format!("Deleted {count} card with duplicate template.").into()
        } else {
            format!("Deleted {count} cards with duplicate template.").into()
        }
    }

    pub fn database_check_missing_decks(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        if count.is_one() {
            format!("Fixed {count} missing deck.").into()
        } else {
            format!("Fixed {count} missing decks.").into()
        }
    }

    pub fn database_check_revlog_properties(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        if count.is_one() {
            format!("Fixed {count} review entry with invalid properties.").into()
        } else {
            format!("Fixed {count} review entries with invalid properties.").into()
        }
    }

    pub fn database_check_notes_with_invalid_utf8(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        if count.is_one() {
            format!("Fixed {count} note with invalid utf8 characters.").into()
        } else {
            format!("Fixed {count} notes with invalid utf8 characters.").into()
        }
    }

    pub fn database_check_fixed_invalid_ids(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        if count.is_one() {
            format!("Fixed {count} object with timestamps in the future.").into()
        } else {
            format!("Fixed {count} objects with timestamps in the future.").into()
        }
    }

    pub fn database_check_notetypes_recovered(&self) -> Cow<'static, str> {
        "One or more note types were missing. The notes that used them have been given new note types starting with \"db-check\", but field names and card design have been lost, so you may be better off restoring from an automatic backup.".into()
    }

    pub fn database_check_checking_integrity(&self) -> Cow<'static, str> {
        "Checking collection...".into()
    }

    pub fn database_check_rebuilding(&self) -> Cow<'static, str> {
        "Rebuilding...".into()
    }

    pub fn database_check_checking_cards(&self) -> Cow<'static, str> {
        "Checking cards...".into()
    }

    pub fn database_check_checking_notes(&self) -> Cow<'static, str> {
        "Checking notes...".into()
    }

    pub fn database_check_checking_history(&self) -> Cow<'static, str> {
        "Checking history...".into()
    }

    pub fn deck_config_default_name(&self) -> Cow<'static, str> {
        "Default".into()
    }

    pub fn deck_config_title(&self) -> Cow<'static, str> {
        "Deck Options".into()
    }

    pub fn deck_config_updating_cards(
        &self,
        current_cards_count: impl Number,
        total_cards_count: impl Number,
    ) -> Cow<'static, str> {
        let current_cards_count = Num::new(current_cards_count);
        let total_cards_count = Num::new(total_cards_count);
        format!("Updating cards: {current_cards_count}/{total_cards_count}...").into()
    }

    pub fn deck_config_invalid_parameters(&self) -> Cow<'static, str> {
        "The provided FSRS parameters are invalid. Leave them blank to use the default values."
            .into()
    }

    pub fn deck_config_not_enough_history(&self) -> Cow<'static, str> {
        "Insufficient review history to perform this operation.".into()
    }

    pub fn deck_config_must_have_400_reviews(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        if count.is_one() {
            format!("Only {count} review was found. You must have at least 400 reviews for this operation.").into()
        } else {
            format!("Only {count} reviews were found. You must have at least 400 reviews for this operation.").into()
        }
    }

    pub fn deck_config_unable_to_determine_desired_retention(&self) -> Cow<'static, str> {
        "Unable to determine a minimum recommended retention.".into()
    }

    pub fn decks_delete_deck(&self) -> Cow<'static, str> {
        "Delete Deck".into()
    }

    pub fn decks_filtered_deck_search_empty(&self) -> Cow<'static, str> {
        "No cards matched the provided search. Some cards may have been excluded because they are in a different filtered deck, or suspended.".into()
    }

    pub fn empty_cards_for_note_type(&self, notetype: impl Display) -> Cow<'static, str> {
        format!("Empty cards for {notetype}:").into()
    }

    pub fn empty_cards_count_line(
        &self,
        empty_count: impl Number,
        existing_count: impl Number,
        template_names: impl Display,
    ) -> Cow<'static, str> {
        let empty_count = Num::new(empty_count);
        let existing_count = Num::new(existing_count);
        format!("{empty_count} of {existing_count} cards empty ({template_names}).").into()
    }

    pub fn errors_parse_number_fail(&self) -> Cow<'static, str> {
        "A number was invalid or out of range.".into()
    }

    pub fn errors_filtered_parent_deck(&self) -> Cow<'static, str> {
        "Filtered decks can not have child decks.".into()
    }

    pub fn errors_filtered_deck_required(&self) -> Cow<'static, str> {
        "This action can only be used on a filtered deck.".into()
    }

    pub fn errors_multiple_notetypes_selected(&self) -> Cow<'static, str> {
        "Please select notes from only one note type.".into()
    }

    pub fn errors_please_check_database(&self) -> Cow<'static, str> {
        "Please use the Check Database action, then try again.".into()
    }

    pub fn errors_please_check_media(&self) -> Cow<'static, str> {
        "Please use the Check Media action, then try again.".into()
    }

    pub fn errors_collection_too_new(&self) -> Cow<'static, str> {
        "This collection requires a newer version of Anki to open.".into()
    }

    pub fn errors_inconsistent_db_state(&self) -> Cow<'static, str> {
        "Your database appears to be in an inconsistent state. Please use the Check Database action.".into()
    }

    pub fn exporting_exporting_file(&self) -> Cow<'static, str> {
        "Exporting file...".into()
    }

    pub fn exporting_processed_media_files(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        if count.is_one() {
            format!("Processed {count} media file...").into()
        } else {
            format!("Processed {count} media files...").into()
        }
    }

    pub fn importing_cannot_merge_notetypes_of_different_kinds(&self) -> Cow<'static, str> {
        "Cloze note types cannot be merged with regular note types.\nYou may still import the file with 'Merge note types' disabled.".into()
    }

    pub fn importing_file_must_contain_field_column(&self) -> Cow<'static, str> {
        "File must contain at least one column that can be mapped to a note field.".into()
    }

    pub fn importing_the_provided_file_is_not_a(&self) -> Cow<'static, str> {
        "The provided file is not a valid .apkg file.".into()
    }

    pub fn importing_processed_media_file(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        if count.is_one() {
            format!("Imported {count} media file").into()
        } else {
            format!("Imported {count} media files").into()
        }
    }

    pub fn importing_importing_file(&self) -> Cow<'static, str> {
        "Importing file...".into()
    }

    pub fn importing_extracting(&self) -> Cow<'static, str> {
        "Extracting data...".into()
    }

    pub fn importing_gathering(&self) -> Cow<'static, str> {
        "Gathering data...".into()
    }

    pub fn importing_failed_to_import_media_file(
        &self,
        debuginfo: impl Display,
    ) -> Cow<'static, str> {
        format!("Failed to import media file: {debuginfo}").into()
    }

    pub fn importing_processed_notes(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        if count.is_one() {
            format!("Processed {count} note...").into()
        } else {
            format!("Processed {count} notes...").into()
        }
    }

    pub fn importing_file_empty(&self) -> Cow<'static, str> {
        "The file you selected is empty.".into()
    }

    pub fn media_check_trash_count(&self, count: impl Number, megs: f32) -> Cow<'static, str> {
        let count = Num::new(count);
        let megs = Num(megs.into());
        if count.is_one() {
            format!("Trash folder: {count} file, {megs}MB").into()
        } else {
            format!("Trash folder: {count} files, {megs}MB").into()
        }
    }

    pub fn media_check_missing_count(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        format!("Missing files: {count}").into()
    }

    pub fn media_check_unused_count(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        format!("Unused files: {count}").into()
    }

    pub fn media_check_renamed_count(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        format!("Renamed files: {count}").into()
    }

    pub fn media_check_oversize_count(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        format!("Over 100MB: {count}").into()
    }

    pub fn media_check_subfolder_count(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        format!("Subfolders: {count}").into()
    }

    pub fn media_check_extracted_count(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        format!("Extracted images: {count}").into()
    }

    pub fn media_check_renamed_header(&self) -> Cow<'static, str> {
        "Some files have been renamed for compatibility:".into()
    }

    pub fn media_check_oversize_header(&self) -> Cow<'static, str> {
        "Files over 100MB are not tracked in the media database.".into()
    }

    pub fn media_check_subfolder_header(&self) -> Cow<'static, str> {
        "Folders inside the media folder are not supported.".into()
    }

    pub fn media_check_missing_header(&self) -> Cow<'static, str> {
        "The following files are referenced by cards, but were not found in the media folder:"
            .into()
    }

    pub fn media_check_unused_header(&self) -> Cow<'static, str> {
        "The following files were found in the media folder, but do not appear to be used on any cards:".into()
    }

    pub fn media_check_template_references_field_header(&self) -> Cow<'static, str> {
        "Anki can not detect used files when you use {{Field}} references in media/LaTeX tags. The media/LaTeX tags should be placed on individual notes instead.\n\nReferencing templates:".into()
    }

    pub fn media_check_renamed_file(
        &self,
        old: impl Display,
        new: impl Display,
    ) -> Cow<'static, str> {
        format!("Renamed: {old} -> {new}").into()
    }

    pub fn media_check_oversize_file(&self, filename: impl Display) -> Cow<'static, str> {
        format!("Over 100MB: {filename}").into()
    }

    pub fn media_check_subfolder_file(&self, filename: impl Display) -> Cow<'static, str> {
        format!("Folder: {filename}").into()
    }

    pub fn media_check_missing_file(&self, filename: impl Display) -> Cow<'static, str> {
        format!("Missing: {filename}").into()
    }

    pub fn media_check_unused_file(&self, filename: impl Display) -> Cow<'static, str> {
        format!("Unused: {filename}").into()
    }

    pub fn media_check_notetype_template(
        &self,
        notetype: impl Display,
        card_type: impl Display,
        side: impl Display,
    ) -> Cow<'static, str> {
        format!("{notetype}: {card_type} ({side})").into()
    }

    pub fn media_check_checked(&self, count: impl Number) -> Cow<'static, str> {
        let count = Num::new(count);
        format!("Checked {count}...").into()
    }

    pub fn notetypes_front_field(&self) -> Cow<'static, str> {
        "Front".into()
    }

    pub fn notetypes_back_field(&self) -> Cow<'static, str> {
        "Back".into()
    }

    pub fn notetypes_add_reverse_field(&self) -> Cow<'static, str> {
        "Add Reverse".into()
    }

    pub fn notetypes_text_field(&self) -> Cow<'static, str> {
        "Text".into()
    }

    pub fn notetypes_back_extra_field(&self) -> Cow<'static, str> {
        "Back Extra".into()
    }

    pub fn notetypes_basic_name(&self) -> Cow<'static, str> {
        "Basic".into()
    }

    pub fn notetypes_basic_reversed_name(&self) -> Cow<'static, str> {
        "Basic (and reversed card)".into()
    }

    pub fn notetypes_basic_optional_reversed_name(&self) -> Cow<'static, str> {
        "Basic (optional reversed card)".into()
    }

    pub fn notetypes_basic_type_answer_name(&self) -> Cow<'static, str> {
        "Basic (type in the answer)".into()
    }

    pub fn notetypes_cloze_name(&self) -> Cow<'static, str> {
        "Cloze".into()
    }

    pub fn notetypes_card_1_name(&self) -> Cow<'static, str> {
        "Card 1".into()
    }

    pub fn notetypes_card_2_name(&self) -> Cow<'static, str> {
        "Card 2".into()
    }

    pub fn scheduling_answer_button_time_seconds(&self, amount: impl Number) -> Cow<'static, str> {
        let amount = Num::new(amount);
        format!("{amount}s").into()
    }

    pub fn scheduling_answer_button_time_minutes(&self, amount: impl Number) -> Cow<'static, str> {
        let amount = Num::new(amount);
        format!("{amount}m").into()
    }

    pub fn scheduling_answer_button_time_hours(&self, amount: impl Number) -> Cow<'static, str> {
        let amount = Num::new(amount);
        format!("{amount}h").into()
    }

    pub fn scheduling_answer_button_time_days(&self, amount: impl Number) -> Cow<'static, str> {
        let amount = Num::new(amount);
        format!("{amount}d").into()
    }

    pub fn scheduling_answer_button_time_months(&self, amount: impl Number) -> Cow<'static, str> {
        let amount = Num::new(amount);
        format!("{amount}mo").into()
    }

    pub fn scheduling_answer_button_time_years(&self, amount: impl Number) -> Cow<'static, str> {
        let amount = Num::new(amount);
        format!("{amount}y").into()
    }

    pub fn scheduling_end(&self) -> Cow<'static, str> {
        "(end)".into()
    }

    pub fn search_invalid_search(&self, reason: impl Display) -> Cow<'static, str> {
        format!("Invalid search: {reason}").into()
    }

    pub fn search_misplaced_and(&self) -> Cow<'static, str> {
        "an `and` was found but it is not connecting two search terms. If you want to search for the word itself, wrap it in double quotes: `\"and\"`.".into()
    }

    pub fn search_misplaced_or(&self) -> Cow<'static, str> {
        "an `or` was found but it is not connecting two search terms. If you want to search for the word itself, wrap it in double quotes: `\"or\"`.".into()
    }

    pub fn search_empty_group(&self) -> Cow<'static, str> {
        "a group `(...)` was found, but there was nothing between the brackets to search for. If you want to search for literal brackets, wrap them in double quotes: `\"( )\"`.".into()
    }

    pub fn search_unopened_group(&self) -> Cow<'static, str> {
        "a closing bracket `)` was found, but there was no opening bracket `(` preceding it. If you want to search for a literal `)`, wrap it in double quotes or prepend a backslash: `\")\"` or `\\)`.".into()
    }

    pub fn search_unclosed_group(&self) -> Cow<'static, str> {
        "an opening bracket `(` was found, but there was no closing bracket `)` following it. If you want to search for a literal `(`, wrap it in double quotes or prepend a backslash: `\"(\"` or `\\(` .".into()
    }

    pub fn search_empty_quote(&self) -> Cow<'static, str> {
        "a pair of double quotes `\"\"` was found, but there was nothing between them to search for. If you want to search for literal double quotes, prepend backslashes: `\\\"\\\"`.".into()
    }

    pub fn search_unclosed_quote(&self) -> Cow<'static, str> {
        "an opening double quote `\"` was found, but there was no second one to close it. If you want to search for a literal `\"`, prepend a backslash: `\\\"`.".into()
    }

    pub fn search_missing_key(&self) -> Cow<'static, str> {
        "a colon `:` was found, but there was no keyword preceding it. If you want to search for a literal `:`, prepend a backslash: `\\:`.".into()
    }

    pub fn search_unknown_escape(&self, val: impl Display) -> Cow<'static, str> {
        format!("the escape sequence `{val}` is not defined. If you want to search for a literal backslash `\\`, prepend another one: `\\\\`.").into()
    }

    pub fn search_invalid_argument(
        &self,
        term: impl Display,
        argument: impl Display,
    ) -> Cow<'static, str> {
        format!("`{term}` was given an invalid argument '`{argument}`'.").into()
    }

    pub fn search_invalid_prop_operator(&self, val: impl Display) -> Cow<'static, str> {
        format!("`prop:{val}` must be followed by one of the following comparison operators: `=`, `!=`, `<`, `>`, `<=` or `>=`.").into()
    }

    pub fn search_invalid_other(&self) -> Cow<'static, str> {
        "please check for typing mistakes.".into()
    }

    pub fn search_invalid_number(
        &self,
        context: impl Display,
        provided: impl Display,
    ) -> Cow<'static, str> {
        format!("expected a number in \"`{context}`\", but found \"`{provided}`\".").into()
    }

    pub fn search_invalid_whole_number(
        &self,
        context: impl Display,
        provided: impl Display,
    ) -> Cow<'static, str> {
        format!("expected a whole number in \"`{context}`\", but found \"`{provided}`\".").into()
    }

    pub fn search_invalid_positive_whole_number(
        &self,
        context: impl Display,
        provided: impl Display,
    ) -> Cow<'static, str> {
        format!("expected a positive whole number in \"`{context}`\", but found \"`{provided}`\".")
            .into()
    }

    pub fn search_invalid_negative_whole_number(
        &self,
        context: impl Display,
        provided: impl Display,
    ) -> Cow<'static, str> {
        format!("expected a whole number less than or equal to 0 in \"`{context}`\", but found \"`{provided}`\".").into()
    }

    pub fn search_invalid_answer_button(
        &self,
        context: impl Display,
        provided: impl Display,
    ) -> Cow<'static, str> {
        format!(
            "expected an answer button between 1-4 in \"`{context}`\", but found \"`{provided}`\"."
        )
        .into()
    }

    pub fn statistics_studied_today(
        &self,
        unit: impl Display,
        secs_per_card: impl Number,
        amount: impl Number,
        cards: impl Number,
    ) -> Cow<'static, str> {
        let cards = Num::new(cards);
        let cards = if cards.is_one() {
            format!("{cards} card")
        } else {
            format!("{cards} cards")
        };
        let amount = Num::new(amount);
        let unit = match unit.to_string().as_str() {
            "seconds" => "second",
            "minutes" => "minute",
            "hours" => "hour",
            "days" => "day",
            "months" => "month",
            _ => "year",
        };
        let plural = if amount.is_one() { "" } else { "s" };
        let secs_per_card = Num::new(secs_per_card);
        format!("Studied {cards}\nin {amount} {unit}{plural} today\n({secs_per_card}s/card)").into()
    }

    pub fn studying_delete_note(&self) -> Cow<'static, str> {
        "Delete Note".into()
    }

    pub fn studying_empty(&self) -> Cow<'static, str> {
        "Empty".into()
    }

    pub fn studying_suspend(&self) -> Cow<'static, str> {
        "Suspend".into()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn numbers() {
        let tr = I18n::new();
        assert_eq!(tr.database_check_missing_decks(1), "Fixed 1 missing deck.");
        assert_eq!(tr.database_check_missing_decks(2), "Fixed 2 missing decks.");
        assert_eq!(
            tr.database_check_missing_decks(1.001),
            "Fixed 1 missing deck."
        );
        assert_eq!(
            tr.database_check_missing_decks(1.01),
            "Fixed 1.01 missing decks."
        );
        assert_eq!(
            tr.media_check_trash_count(1234, 0.5),
            "Trash folder: 1234 files, 0.5MB"
        );
    }
}
