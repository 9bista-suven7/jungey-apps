#!/usr/bin/env python3
"""Package the Jungey colour theme for VS Code as a .vsix and install it."""
import json
import subprocess
import zipfile
from pathlib import Path

import palette as P

HERE = Path(__file__).parent
OUT = HERE / "jungey-theme.vsix"

INK, BASE, RAISED, LINE = P.VOID, P.BASE, P.RAISED, P.LINE
TEXT, MUTED, DIM = P.TEXT, P.MUTED, P.DIM
# C: gold  C2: bright gold  V: pale teal  M: mint  A: amber  R: rose
C, C2, V, M, A, R = P.ACCENT, P.ACCENT_HI, P.SECOND, P.SUCCESS, P.WARNING, P.ERROR

colors = {
    "focusBorder": C + "66", "foreground": TEXT, "descriptionForeground": MUTED,
    "selection.background": C + "40", "widget.shadow": "#00000080", "icon.foreground": MUTED,
    "editor.background": BASE, "editor.foreground": TEXT,
    "editor.lineHighlightBackground": "#ffffff08", "editor.lineHighlightBorder": "#00000000",
    "editor.selectionBackground": C + "33", "editor.inactiveSelectionBackground": C + "1a",
    "editor.findMatchBackground": A + "55", "editor.findMatchHighlightBackground": A + "26",
    "editor.wordHighlightBackground": V + "26", "editorCursor.foreground": C,
    "editorLineNumber.foreground": DIM, "editorLineNumber.activeForeground": C,
    "editorIndentGuide.background1": LINE, "editorIndentGuide.activeBackground1": C + "55",
    "editorBracketMatch.background": C + "1a", "editorBracketMatch.border": C + "80",
    "editorWhitespace.foreground": LINE, "editorRuler.foreground": LINE,
    "editorGutter.addedBackground": M, "editorGutter.modifiedBackground": C, "editorGutter.deletedBackground": R,
    "editorError.foreground": R, "editorWarning.foreground": A, "editorInfo.foreground": C,
    "editorWidget.background": RAISED, "editorWidget.border": LINE, "editorSuggestWidget.selectedBackground": C + "26",
    "editorHoverWidget.background": RAISED, "editorHoverWidget.border": LINE,
    "editorGroupHeader.tabsBackground": INK, "editorGroup.border": LINE,
    "tab.activeBackground": BASE, "tab.activeForeground": TEXT, "tab.activeBorderTop": C,
    "tab.inactiveBackground": INK, "tab.inactiveForeground": DIM, "tab.border": INK, "tab.hoverBackground": RAISED,
    "titleBar.activeBackground": INK, "titleBar.activeForeground": TEXT,
    "titleBar.inactiveBackground": INK, "titleBar.inactiveForeground": DIM,
    "activityBar.background": INK, "activityBar.foreground": C, "activityBar.inactiveForeground": DIM,
    "activityBar.activeBorder": C, "activityBarBadge.background": P.ACCENT_BG, "activityBarBadge.foreground": P.ACCENT_FG,
    "sideBar.background": INK, "sideBar.foreground": MUTED, "sideBar.border": LINE,
    "sideBarTitle.foreground": TEXT, "sideBarSectionHeader.background": INK, "sideBarSectionHeader.foreground": TEXT,
    "list.activeSelectionBackground": C + "26", "list.activeSelectionForeground": TEXT,
    "list.inactiveSelectionBackground": C + "14", "list.hoverBackground": "#ffffff0a",
    "list.highlightForeground": C, "list.focusOutline": C + "66",
    "statusBar.background": INK, "statusBar.foreground": MUTED, "statusBar.border": LINE,
    "statusBar.debuggingBackground": V, "statusBar.debuggingForeground": INK,
    "statusBar.noFolderBackground": INK, "statusBarItem.remoteBackground": P.ACCENT_BG, "statusBarItem.remoteForeground": P.ACCENT_FG,
    "panel.background": INK, "panel.border": LINE, "panelTitle.activeBorder": C, "panelTitle.activeForeground": TEXT,
    "terminal.background": BASE, "terminal.foreground": TEXT, "terminalCursor.foreground": C,
    "terminal.ansiBlack": BASE, "terminal.ansiRed": "#f43f5e", "terminal.ansiGreen": M, "terminal.ansiYellow": A,
    "terminal.ansiBlue": "#818cf8", "terminal.ansiMagenta": V, "terminal.ansiCyan": C, "terminal.ansiWhite": TEXT,
    "terminal.ansiBrightBlack": DIM, "terminal.ansiBrightRed": R, "terminal.ansiBrightGreen": "#7dffbf",
    "terminal.ansiBrightYellow": "#ffe08a", "terminal.ansiBrightBlue": "#a5b4fc", "terminal.ansiBrightMagenta": "#c4b5fd",
    "terminal.ansiBrightCyan": C2, "terminal.ansiBrightWhite": "#f2fbff",
    "input.background": RAISED, "input.border": LINE, "input.placeholderForeground": DIM,
    "inputOption.activeBorder": C, "dropdown.background": RAISED, "dropdown.border": LINE,
    "button.background": P.ACCENT_BG, "button.foreground": P.ACCENT_FG, "button.hoverBackground": C2,
    "button.secondaryBackground": RAISED, "button.secondaryForeground": TEXT,
    "badge.background": P.ACCENT_BG, "badge.foreground": P.ACCENT_FG, "progressBar.background": C,
    "scrollbarSlider.background": "#ffffff14", "scrollbarSlider.hoverBackground": "#ffffff24",
    "scrollbarSlider.activeBackground": C + "55",
    "textLink.foreground": C, "textLink.activeForeground": C2,
    "gitDecoration.modifiedResourceForeground": C, "gitDecoration.untrackedResourceForeground": M,
    "gitDecoration.deletedResourceForeground": R, "gitDecoration.ignoredResourceForeground": DIM,
    "breadcrumb.foreground": DIM, "breadcrumb.focusForeground": TEXT,
    "minimap.background": BASE, "notifications.background": RAISED, "notifications.border": LINE,
    "quickInput.background": RAISED, "quickInputList.focusBackground": C + "26",
    "menu.background": RAISED, "menu.selectionBackground": C + "26", "menu.border": LINE,
}

def rule(scope, fg, style=None):
    s = {"foreground": fg}
    if style:
        s["fontStyle"] = style
    return {"scope": scope, "settings": s}

tokens = [
    rule(["comment", "punctuation.definition.comment"], DIM, "italic"),
    rule(["keyword", "storage", "storage.type", "keyword.control", "keyword.operator.new",
          "keyword.operator.expression", "keyword.operator.logical.python"], C),
    rule(["string", "string.quoted", "string.template", "markup.inline.raw"], M),
    rule(["constant.numeric", "constant.language", "constant.character", "support.constant",
          "variable.other.constant", "variable.other.enummember"], A),
    rule(["entity.name.function", "support.function", "meta.function-call", "variable.function"], V),
    rule(["entity.name.type", "entity.name.class", "support.class", "support.type",
          "entity.other.inherited-class", "storage.type.java", "storage.type.primitive"], C2),
    rule(["variable", "variable.other", "meta.definition.variable"], TEXT),
    rule(["variable.parameter"], "#b9e9f7", "italic"),
    rule(["variable.language", "variable.language.this", "variable.language.self"], R, "italic"),
    rule(["entity.name.tag", "punctuation.definition.tag"], R),
    rule(["entity.other.attribute-name"], A, "italic"),
    rule(["support.type.property-name", "meta.object-literal.key", "variable.other.property",
          "variable.other.object.property"], "#b9e9f7"),
    rule(["keyword.operator", "punctuation", "meta.brace"], MUTED),
    rule(["entity.name.function.decorator", "meta.decorator", "storage.type.annotation",
          "punctuation.decorator"], A),
    rule(["markup.heading", "entity.name.section"], C, "bold"),
    rule(["markup.bold"], TEXT, "bold"), rule(["markup.italic"], TEXT, "italic"),
    rule(["markup.underline.link", "string.other.link"], C),
    rule(["markup.inserted"], M), rule(["markup.deleted"], R), rule(["markup.changed"], A),
    rule(["invalid"], R),
]

semantic = {
    "function": V, "method": V, "class": C2, "type": C2, "interface": C2, "enum": C2,
    "parameter": {"foreground": "#b9e9f7", "italic": True}, "property": "#b9e9f7",
    "enumMember": A, "decorator": A, "namespace": C2, "*.readonly": A,
}

theme = {"name": "Jungey", "type": "dark", "semanticHighlighting": True,
         "colors": colors, "tokenColors": tokens, "semanticTokenColors": semantic}

package = {
    "name": "jungey-theme", "displayName": "Jungey Theme", "publisher": "jungey", "version": "3.0.0",
    "description": "Deep teal and olive gold, to match Jungey OS.",
    "engines": {"vscode": "^1.70.0"}, "categories": ["Themes"],
    "contributes": {"themes": [{"label": "Jungey", "uiTheme": "vs-dark", "path": "./themes/jungey.json"}]},
}

manifest = """<?xml version="1.0" encoding="utf-8"?>
<PackageManifest Version="3.0.0" xmlns="http://schemas.microsoft.com/developer/vsx-schema/2011" xmlns:d="http://schemas.microsoft.com/developer/vsx-schema-design/2011">
  <Metadata>
    <Identity Language="en-US" Id="jungey-theme" Version="3.0.0" Publisher="jungey"/>
    <DisplayName>Jungey Theme</DisplayName>
    <Description xml:space="preserve">Deep teal and olive gold, to match Jungey OS.</Description>
    <Categories>Themes</Categories>
    <Properties><Property Id="Microsoft.VisualStudio.Code.Engine" Value="^1.70.0"/></Properties>
  </Metadata>
  <Installation><InstallationTarget Id="Microsoft.VisualStudio.Code"/></Installation>
  <Dependencies/>
  <Assets><Asset Type="Microsoft.VisualStudio.Code.Manifest" Path="extension/package.json" Addressable="true"/></Assets>
</PackageManifest>
"""

content_types = """<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension=".json" ContentType="application/json"/>
  <Default Extension=".vsixmanifest" ContentType="text/xml"/>
</Types>
"""

with zipfile.ZipFile(OUT, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("[Content_Types].xml", content_types)
    z.writestr("extension.vsixmanifest", manifest)
    z.writestr("extension/package.json", json.dumps(package, indent=2))
    z.writestr("extension/themes/jungey.json", json.dumps(theme, indent=2))

subprocess.run(["code", "--install-extension", str(OUT), "--force"], check=True)
