# Git Branch Favorites

Use the plus beside an unfavorited branch in the bottom-right branch switcher or Git Graph's branch menu to add it to Favorites. Favorite branches show a star; click it to remove the favorite. Favorites appear first in both branch menus. Toggling a favorite keeps the menu open and does not check out the branch. Git Graph branch badges display a star only for favorites; click that star to remove the favorite.

Favorites are saved in local workbench preferences per project and source-control root. Linked worktrees of the same project share favorites; nested repositories and other projects keep separate lists.

Choose **Favorite Branches** in Git Graph's branch perspective menu to show only favorite branch tips and their ancestors. Shared ancestry and merge parents remain connected, and pagination counts only the selected history. This also works with file history. A graph with no existing favorite branches shows an empty state. The selected perspective is saved with the graph tab; changing the shared favorites refreshes an open favorites-only graph.

The **Merged Into** selector appears only when a merged-branch hiding mode is active for a view containing multiple branches. **Show All** hides that selector while retaining its saved target for later use.
