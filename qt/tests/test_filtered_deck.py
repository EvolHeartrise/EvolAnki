# Copyright: Ankitects Pty Ltd and contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

from types import SimpleNamespace
from unittest.mock import MagicMock, patch

from anki.decks import FilteredDeckConfig
from aqt.filtered_deck import FilteredDeckConfigDialog

Order = FilteredDeckConfig.SearchTerm.Order


@patch("aqt.filtered_deck.restoreGeom")
@patch("aqt.filtered_deck.disable_help_button")
@patch("aqt.filtered_deck.qconnect")
@patch("aqt.filtered_deck.theme_manager")
@patch("aqt.filtered_deck.tr")
@patch("aqt.filtered_deck.aqt.forms.filtered_deck.Ui_Dialog")
def test_initial_setup_offers_every_order(
    ui_dialog: MagicMock,
    _tr: MagicMock,
    _theme_manager: MagicMock,
    _qconnect: MagicMock,
    _disable_help_button: MagicMock,
    _restore_geom: MagicMock,
) -> None:
    labels = list(Order.keys())
    dialog = MagicMock()
    dialog.col.sched.filtered_deck_order_labels.return_value = labels
    dialog.GEOMETRY_KEY = FilteredDeckConfigDialog.GEOMETRY_KEY
    form = ui_dialog.return_value

    FilteredDeckConfigDialog._initial_dialog_setup(dialog)

    form.order.addItems.assert_called_once_with(labels)
    form.order_2.addItems.assert_called_once_with(labels)
    offered = form.order.addItems.call_args.args[0]
    assert offered[Order.RETRIEVABILITY_ASCENDING] == "RETRIEVABILITY_ASCENDING"
    assert offered[Order.RETRIEVABILITY_DESCENDING] == "RETRIEVABILITY_DESCENDING"


@patch("aqt.filtered_deck.without_unicode_isolation", return_value="title")
@patch("aqt.filtered_deck.tr")
def test_load_deck_maps_saved_orders_to_combo_rows(
    _tr: MagicMock, _without_unicode_isolation: MagicMock
) -> None:
    dialog = MagicMock()
    dialog.form = MagicMock()
    dialog.DEFAULT_ORDER = FilteredDeckConfigDialog.DEFAULT_ORDER
    dialog.deck = SimpleNamespace(
        id=1,
        name="Filtered",
        config=FilteredDeckConfig(
            reschedule=True,
            search_terms=[
                FilteredDeckConfig.SearchTerm(
                    search="deck:Default",
                    limit=100,
                    order=Order.RETRIEVABILITY_ASCENDING,
                ),
                FilteredDeckConfig.SearchTerm(
                    search="is:due",
                    limit=20,
                    order=Order.RELATIVE_OVERDUENESS,
                ),
            ],
        ),
    )

    FilteredDeckConfigDialog._load_deck(dialog)

    dialog.form.order.setCurrentIndex.assert_called_once_with(
        Order.RETRIEVABILITY_ASCENDING
    )
    dialog.form.order_2.setCurrentIndex.assert_called_once_with(
        Order.RELATIVE_OVERDUENESS
    )

    dialog.deck.config.search_terms.pop()
    dialog.form.order_2.setCurrentIndex.reset_mock()
    FilteredDeckConfigDialog._load_deck(dialog)

    dialog.form.order_2.setCurrentIndex.assert_called_once_with(Order.RANDOM)


def test_update_deck_maps_combo_rows_back_to_orders() -> None:
    dialog = MagicMock()
    dialog.form = MagicMock()
    dialog.deck = SimpleNamespace(
        name="Filtered",
        config=FilteredDeckConfig(),
    )

    dialog.form.order.currentIndex.return_value = Order.RETRIEVABILITY_DESCENDING
    dialog.form.order_2.currentIndex.return_value = Order.RANDOM
    dialog.form.secondFilter.isChecked.return_value = True
    dialog.form.search.text.return_value = "deck:Default"
    dialog.form.limit.value.return_value = 100
    dialog.form.search_2.text.return_value = "is:due"
    dialog.form.limit_2.value.return_value = 20
    dialog.form.resched.isChecked.return_value = True
    dialog.form.preview_again.value.return_value = 60
    dialog.form.preview_hard.value.return_value = 600
    dialog.form.preview_good.value.return_value = 0

    assert FilteredDeckConfigDialog._update_deck(dialog)

    terms = dialog.deck.config.search_terms
    assert terms[0].order == Order.RETRIEVABILITY_DESCENDING
    assert terms[1].order == Order.RANDOM
