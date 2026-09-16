import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import "../theme"

ColumnLayout {
    id: root
    spacing: 8
    Layout.fillWidth: true

    property string activePlatform: ""

    signal triggerSocialOptimize(string platformCode)
    signal resetSocial()

    RowLayout {
        Layout.fillWidth: true
        spacing: 4

        Text {
            text: "AI SOCIAL MEDIA OPTIMIZER"
            textFormat: Text.PlainText
            font.pixelSize: 10
            font.weight: Font.DemiBold
            font.letterSpacing: 1
            color: Theme.accentCyan
            Layout.fillWidth: true
        }

        // Reset social preset button (always accessible)
        Rectangle {
            visible: true
            implicitWidth: 46
            implicitHeight: 18
            radius: Theme.radiusSm
            color: resetSocialMouse.containsMouse ? Qt.rgba(Theme.accentCyan.r, Theme.accentCyan.g, Theme.accentCyan.b, 0.2) : (root.activePlatform !== "" ? Qt.rgba(Theme.accentCyan.r, Theme.accentCyan.g, Theme.accentCyan.b, 0.1) : "transparent")
            border.color: root.activePlatform !== "" ? Theme.accentCyan : Theme.border
            border.width: 1

            Text {
                anchors.centerIn: parent
                text: "RESET"
                textFormat: Text.PlainText
                font.pixelSize: 8
                font.weight: Font.DemiBold
                color: root.activePlatform !== "" ? Theme.accentCyan : Theme.textDim
            }

            MouseArea {
                id: resetSocialMouse
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                hoverEnabled: true
                onClicked: {
                    root.activePlatform = ""
                    root.resetSocial()
                }
            }
        }
    }

    // Grid of Social Media Platform Targets
    GridLayout {
        Layout.fillWidth: true
        columns: 2
        rowSpacing: 6
        columnSpacing: 6

        property var platforms: [
            { id: "ig", name: "Instagram Feed", aspect: "4:5 / 1080x1350", tag: "Max Screen", col: Theme.accentMagenta },
            { id: "story", name: "Stories / Reels", aspect: "9:16 / 1080x1920", tag: "Full Mobile", col: Theme.accentOrange },
            { id: "x", name: "X / Twitter", aspect: "16:9 / 1200x675", tag: "Crisp Feed", col: Theme.accent },
            { id: "ig_square", name: "Classic Square", aspect: "1:1 / 1080x1080", tag: "Grid Post", col: Theme.accentYellow },
            { id: "fb", name: "Facebook HD", aspect: "1.91:1 / 2048px", tag: "High Res", col: Theme.accentCyan },
            { id: "yt", name: "YouTube Thumb", aspect: "16:9 / 1280x720", tag: "High CTR", col: Theme.accentGreen }
        ]

        Repeater {
            model: parent.platforms
            delegate: Rectangle {
                Layout.fillWidth: true
                implicitHeight: 46
                radius: Theme.radiusSm
                color: root.activePlatform === modelData.id
                       ? Qt.rgba(modelData.col.r, modelData.col.g, modelData.col.b, 0.22)
                       : (optMouse.containsMouse ? Theme.bgCardHover : Theme.bgCard)
                border.color: root.activePlatform === modelData.id
                              ? modelData.col
                              : (optMouse.containsMouse ? modelData.col : Theme.border)
                border.width: 1

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 6
                    spacing: 2

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 4
                        Text {
                            text: modelData.name
                            textFormat: Text.PlainText
                            font.pixelSize: 10
                            font.weight: Font.Bold
                            color: root.activePlatform === modelData.id ? modelData.col : Theme.textMain
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                        }
                        Rectangle {
                            implicitWidth: tagText.implicitWidth + 6
                            implicitHeight: 14
                            radius: 3
                            color: Qt.rgba(modelData.col.r, modelData.col.g, modelData.col.b, 0.2)
                            Text {
                                id: tagText
                                anchors.centerIn: parent
                                text: modelData.tag
                                textFormat: Text.PlainText
                                font.pixelSize: 8
                                font.weight: Font.Bold
                                color: modelData.col
                            }
                        }
                    }

                    Text {
                        text: modelData.aspect
                        textFormat: Text.PlainText
                        font.pixelSize: 9
                        font.family: Theme.monoFont
                        color: Theme.textMuted
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                }

                MouseArea {
                    id: optMouse
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    hoverEnabled: true
                    onClicked: {
                        root.activePlatform = modelData.id;
                        root.triggerSocialOptimize(modelData.id);
                    }
                }
            }
        }
    }
}
