import QtQuick
import QtQuick.Layouts
import "../theme"

ColumnLayout {
    id: root
    spacing: 8
    Layout.fillWidth: true

    property string activePreset: ""

    signal applyPreset(string name)
    signal triggerAiAuto()
    signal resetPreset()

    RowLayout {
        Layout.fillWidth: true
        spacing: 4

        Text {
            text: "FILM LOOKS & STYLES"
            textFormat: Text.PlainText
            font.pixelSize: 10
            font.weight: Font.DemiBold
            font.letterSpacing: 1
            color: Theme.textDim
            Layout.fillWidth: true
        }

        // Reset preset button (always accessible)
        Rectangle {
            visible: true
            implicitWidth: 46
            implicitHeight: 18
            radius: Theme.radiusSm
            color: resetPresetMouse.containsMouse ? Qt.rgba(Theme.accentMagenta.r, Theme.accentMagenta.g, Theme.accentMagenta.b, 0.2) : (root.activePreset !== "" ? Qt.rgba(Theme.accentMagenta.r, Theme.accentMagenta.g, Theme.accentMagenta.b, 0.1) : "transparent")
            border.color: root.activePreset !== "" ? Theme.accentMagenta : Theme.border
            border.width: 1

            Text {
                anchors.centerIn: parent
                text: "RESET"
                textFormat: Text.PlainText
                font.pixelSize: 8
                font.weight: Font.DemiBold
                color: root.activePreset !== "" ? Theme.accentMagenta : Theme.textDim
            }

            MouseArea {
                id: resetPresetMouse
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                hoverEnabled: true
                onClicked: {
                    root.activePreset = ""
                    root.resetPreset()
                }
            }
        }
    }

    // AI Auto Card (Special Highlight)
    Rectangle {
        Layout.fillWidth: true
        implicitHeight: 38
        radius: Theme.radiusSm
        color: Qt.rgba(Theme.accentPurple.r, Theme.accentPurple.g, Theme.accentPurple.b, 0.2)
        border.color: Theme.accentPurple
        border.width: 1

        RowLayout {
            anchors.fill: parent
            anchors.margins: 8
            spacing: 8

            Text {
                text: Theme.iconAi
                font.family: Theme.iconFont
                font.pixelSize: 12
                color: Theme.accentPurple
            }

            ColumnLayout {
                spacing: 0
                Layout.fillWidth: true
                Text {
                    text: "AI Magic Auto Enhance"
                    textFormat: Text.PlainText
                    font.pixelSize: 12
                    font.weight: Font.Bold
                    color: Theme.textMain
                }
                Text {
                    text: "Auto Exposure, Dynamic Range & WB"
                    textFormat: Text.PlainText
                    font.pixelSize: 9
                    color: Theme.accentPurple
                }
            }

            Text {
                text: "RUN"
                textFormat: Text.PlainText
                font.pixelSize: 10
                font.weight: Font.Bold
                color: Theme.accentPurple
            }
        }

        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            hoverEnabled: true
            onClicked: {
                root.triggerAiAuto()
            }
        }
    }

    // Grid of visual film simulations
    GridLayout {
        Layout.fillWidth: true
        columns: 2
        rowSpacing: 6
        columnSpacing: 6

        property var presets: [
            { name: "Fuji Classic Chrome", sub: "Documentary Muted", col: Theme.accent },
            { name: "Fuji Velvia 50", sub: "Vivid Landscapes", col: Theme.accentGreen },
            { name: "Kodak Portra 400", sub: "Warm Skin Tones", col: Theme.accentOrange },
            { name: "Leica Monochrom HC", sub: "High Contrast B&W", col: Theme.textMain },
            { name: "Cinematic Teal & Orange", sub: "Film Grade", col: Theme.accentCyan }
        ]

        Repeater {
            model: parent.presets
            delegate: Rectangle {
                Layout.fillWidth: true
                implicitHeight: 38
                radius: Theme.radiusSm
                color: root.activePreset === modelData.name ? Qt.rgba(modelData.col.r, modelData.col.g, modelData.col.b, 0.25) : Theme.bgCard
                border.color: root.activePreset === modelData.name ? modelData.col : Theme.border
                border.width: 1

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 6
                    spacing: 1

                    Text {
                        text: modelData.name
                        textFormat: Text.PlainText
                        font.pixelSize: 11
                        font.weight: Font.DemiBold
                        color: root.activePreset === modelData.name ? modelData.col : Theme.textMain
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }

                    Text {
                        text: modelData.sub
                        textFormat: Text.PlainText
                        font.pixelSize: 9
                        color: Theme.textMuted
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    hoverEnabled: true
                    onClicked: {
                        root.activePreset = modelData.name
                        root.applyPreset(modelData.name)
                    }
                }
            }
        }
    }
}
