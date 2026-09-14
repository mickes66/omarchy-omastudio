import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import "../theme"

ColumnLayout {
    id: root
    spacing: 8
    Layout.fillWidth: true

    property var hslHue: [0, 0, 0, 0, 0, 0, 0, 0]
    property var hslSat: [0, 0, 0, 0, 0, 0, 0, 0]
    property var hslLum: [0, 0, 0, 0, 0, 0, 0, 0]

    property int activeChannel: 0

    signal colorChanged()

    function resetAll() {
        for (var i = 0; i < 8; i++) {
            hslHue[i] = 0;
            hslSat[i] = 0;
            hslLum[i] = 0;
        }
        root.colorChanged();
    }

    readonly property var channelNames: ["Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta"]
    readonly property var channelColors: [
        "#f7768e", "#ff9e64", "#e0af68", "#9ece6a",
        "#7dcfff", "#7aa2f7", "#bb9af7", "#f778ba"
    ]

    // Color Band Selector Pills
    RowLayout {
        Layout.fillWidth: true
        spacing: 4

        Repeater {
            model: 8
            delegate: Rectangle {
                Layout.fillWidth: true
                implicitHeight: 24
                radius: 4
                color: root.activeChannel === index ? root.channelColors[index] : Theme.bgCard
                border.color: root.channelColors[index]
                border.width: root.activeChannel === index ? 0 : 1

                Rectangle {
                    width: 8
                    height: 8
                    radius: 4
                    anchors.centerIn: parent
                    color: root.activeChannel === index ? Theme.bgBase : root.channelColors[index]
                }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        root.activeChannel = index
                    }
                }
            }
        }
    }

    // Active color title
    RowLayout {
        Layout.fillWidth: true
        Text {
            text: root.channelNames[root.activeChannel] + " Channel"
            textFormat: Text.PlainText
            font.pixelSize: 11
            font.weight: Font.DemiBold
            color: root.channelColors[root.activeChannel]
        }
        Item { Layout.fillWidth: true }
        Text {
            text: "Double click to reset"
            textFormat: Text.PlainText
            font.pixelSize: 10
            color: Theme.textDim
        }
    }

    SliderGroup {
        title: "Hue Shift"
        from: -100.0
        to: 100.0
        value: root.hslHue[root.activeChannel]
        accentColor: root.channelColors[root.activeChannel]
        onSliderMoved: function(newVal) {
            root.hslHue[root.activeChannel] = newVal
            root.colorChanged()
        }
    }

    SliderGroup {
        title: "Saturation"
        from: -100.0
        to: 100.0
        value: root.hslSat[root.activeChannel]
        accentColor: root.channelColors[root.activeChannel]
        onSliderMoved: function(newVal) {
            root.hslSat[root.activeChannel] = newVal
            root.colorChanged()
        }
    }

    SliderGroup {
        title: "Luminance"
        from: -100.0
        to: 100.0
        value: root.hslLum[root.activeChannel]
        accentColor: root.channelColors[root.activeChannel]
        onSliderMoved: function(newVal) {
            root.hslLum[root.activeChannel] = newVal
            root.colorChanged()
        }
    }
}
