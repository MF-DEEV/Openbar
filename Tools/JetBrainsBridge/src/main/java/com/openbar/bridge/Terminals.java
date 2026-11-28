// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

package com.openbar.bridge;

import java.io.IOException;
import java.util.List;
import java.util.Set;

/** The IDE's terminal tabs, as the bridge sees them; the IDE's own code stays behind it. */
interface Terminals {
    /** The tab whose shell is one of {@code shellPids}, or null when no open tab has one. */
    Terminal find(Set<Long> shellPids);

    /** The folders of the projects open in this IDE. */
    List<String> projects();

    /** The IDE's name, as the person knows it ("PyCharm"). */
    String ide();

    /** One terminal tab of one project window. */
    interface Terminal {
        /** Brings the tab's project window forward and selects the tab in it. */
        void focus();

        /**
         * Whether the tab is what the person looks at: its window is the
         * IDE's active one, the terminal tool window is open in it and the
         * tab is the one selected there.
         */
        boolean shown();

        /** Bytes to the shell as if typed: a return, a key. */
        void type(String text) throws IOException;

        /** Text to the shell as a paste, so its line breaks stay text. */
        void paste(String text) throws IOException;
    }
}
