import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import "../theme"

ColumnLayout {
    id: root
    spacing: 8
    Layout.fillWidth: true

    property var liftRgb: [0.0, 0.0, 0.0]
    property real liftLuma: 0.0

    property var gammaRgb: [0.0, 0.0, 0.0]
    property real gammaLuma: 0.0

    property var gainRgb: [0.0, 0.0, 0.0]
    property real gainLuma: 0.0

    property var offsetRgb: [0.0, 0.0, 0.0]
    property real offsetLuma: 0.0

    property int activeTab: 0 // 0: Lift, 1: Gamma, 2: Gain, 3: Offset

    signal gradingChanged()

    function resetAllWheels() {
        liftRgb = [0.0, 0.0, 0.0];
        liftLuma = 0.0;
        gammaRgb = [0.0, 0.0, 0.0];
        gammaLuma = 0.0;
        gainRgb = [0.0, 0.0, 0.0];
        gainLuma = 0.0;
        offsetRgb = [0.0, 0.0, 0.0];
        offsetLuma = 0.0;
        liftWheel.resetWheel();
        gammaWheel.resetWheel();
        gainWheel.resetWheel();
        offsetWheel.resetWheel();
        root.gradingChanged();
    }

    // Header with Title and Reset All
    RowLayout {
        Layout.fillWidth: true
        spacing: 6

        Text {
            text: "DAVINCI 3-WAY COLOR WHEELS"
            textFormat: Text.PlainText
            font.pixelSize: 10
            font.weight: Font.DemiBold
            font.letterSpacing: 1
            color: Theme.textDim
            Layout.fillWidth: true
        }

        Rectangle {
            implicitWidth: resetAllText.implicitWidth + 10
            implicitHeight: 18
            radius: 4
            color: resetAllMouse.containsMouse ? Theme.bgCardHover : "transparent"
            border.color: Theme.border
            border.width: 1

            Text {
                id: resetAllText
                anchors.centerIn: parent
                text: "RESET ALL"
                textFormat: Text.PlainText
                font.pixelSize: 8
                font.weight: Font.Bold
                color: Theme.textDim
            }

            MouseArea {
                id: resetAllMouse
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                hoverEnabled: true
                onClicked: root.resetAllWheels()
            }
        }
    }

    // Tab Switcher (Lift, Gamma, Gain, Offset)
    RowLayout {
        Layout.fillWidth: true
        spacing: 4

        property var tabs: [
            { name: "LIFT", sub: "Shadows", col: Theme.accentCyan },
            { name: "GAMMA", sub: "Mids", col: Theme.accentYellow },
            { name: "GAIN", sub: "Highlights", col: Theme.accentOrange },
            { name: "OFFSET", sub: "Global", col: Theme.accentPurple }
        ]

        Repeater {
            model: parent.tabs
            delegate: Rectangle {
                Layout.fillWidth: true
                implicitHeight: 28
                radius: 4
                property bool isCur: root.activeTab === index
                color: isCur ? Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.2) : Theme.bgCard
                border.color: isCur ? modelData.col : Theme.border
                border.width: 1

                ColumnLayout {
                    anchors.centerIn: parent
                    spacing: 0
                    Text {
                        text: modelData.name
                        textFormat: Text.PlainText
                        font.pixelSize: 10
                        font.weight: Font.Bold
                        color: parent.parent.isCur ? modelData.col : Theme.textMain
                        horizontalAlignment: Text.AlignHCenter
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.activeTab = index
                }
            }
        }
    }

    // Wheels Display Container
    Rectangle {
        Layout.fillWidth: true
        implicitHeight: 175
        radius: Theme.radiusSm
        color: Theme.bgCard
        border.color: Theme.border
        border.width: 1

        Item {
            anchors.fill: parent
            anchors.margins: 10

            // 1. Lift Wheel (Shadows)
            ColorWheel {
                id: liftWheel
                anchors.fill: parent
                visible: root.activeTab === 0
                title: "Shadows Lift"
                rgbOffset: root.liftRgb
                lumaOffset: root.liftLuma
                wheelAccent: Theme.accentCyan
                onWheelChanged: function(rgb, luma) {
                    root.liftRgb = rgb;
                    root.liftLuma = luma;
                    root.gradingChanged();
                }
            }

            // 2. Gamma Wheel (Midtones)
            ColorWheel {
                id: gammaWheel
                anchors.fill: parent
                visible: root.activeTab === 1
                title: "Midtones Gamma"
                rgbOffset: root.gammaRgb
                lumaOffset: root.gammaLuma
                wheelAccent: Theme.accentYellow
                onWheelChanged: function(rgb, luma) {
                    root.gammaRgb = rgb;
                    root.gammaLuma = luma;
                    root.gradingChanged();
                }
            }

            // 3. Gain Wheel (Highlights)
            ColorWheel {
                id: gainWheel
                anchors.fill: parent
                visible: root.activeTab === 2
                title: "Highlights Gain"
                rgbOffset: root.gainRgb
                lumaOffset: root.gainLuma
                wheelAccent: Theme.accentOrange
                onWheelChanged: function(rgb, luma) {
                    root.gainRgb = rgb;
                    root.gainLuma = luma;
                    root.gradingChanged();
                }
            }

            // 4. Offset Wheel (Global)
            ColorWheel {
                id: offsetWheel
                anchors.fill: parent
                visible: root.activeTab === 3
                title: "Master Offset"
                rgbOffset: root.offsetRgb
                lumaOffset: root.offsetLuma
                wheelAccent: Theme.accentPurple
                onWheelChanged: function(rgb, luma) {
                    root.offsetRgb = rgb;
                    root.offsetLuma = luma;
                    root.gradingChanged();
                }
            }
        }
    }
}
