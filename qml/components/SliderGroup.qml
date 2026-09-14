import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import "../theme"

ColumnLayout {
    id: root
    spacing: 4
    Layout.fillWidth: true

    property string title: "Adjustment"
    property real from: -100.0
    property real to: 100.0
    property real value: 0.0
    property real defaultValue: 0.0
    property real stepSize: 1.0
    property string suffix: ""
    property int decimals: 0
    property color accentColor: Theme.accent

    signal sliderMoved(real newValue)

    RowLayout {
        Layout.fillWidth: true
        spacing: 8

        Text {
            text: root.title
            textFormat: Text.PlainText
            color: Theme.textMain
            font.pixelSize: 12
            font.weight: Font.Medium
            Layout.fillWidth: true
            elide: Text.ElideRight
        }

        Rectangle {
            implicitWidth: valText.implicitWidth + 12
            implicitHeight: 20
            radius: 4
            color: root.value !== root.defaultValue ? Qt.rgba(root.accentColor.r, root.accentColor.g, root.accentColor.b, 0.2) : Theme.bgCard
            border.color: root.value !== root.defaultValue ? root.accentColor : Theme.border
            border.width: 1

            Text {
                id: valText
                anchors.centerIn: parent
                text: (root.decimals === 0 ? Math.round(root.value) : root.value.toFixed(root.decimals)) + root.suffix
                textFormat: Text.PlainText
                color: root.value !== root.defaultValue ? root.accentColor : Theme.textMuted
                font.pixelSize: 11
                font.family: Theme.monoFont
                font.weight: Font.DemiBold
            }

            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onDoubleClicked: {
                    root.value = root.defaultValue
                    root.sliderMoved(root.defaultValue)
                }
            }
        }
    }

    Slider {
        id: slider
        Layout.fillWidth: true
        from: root.from
        to: root.to
        stepSize: root.stepSize
        value: root.value

        background: Rectangle {
            x: slider.leftPadding
            y: slider.topPadding + slider.availableHeight / 2 - height / 2
            implicitWidth: 200
            implicitHeight: 4
            width: slider.availableWidth
            height: implicitHeight
            radius: 2
            color: Theme.bgCard

            Rectangle {
                // Fill from center if bidirectional, or from left
                property real zeroPos: root.from < 0 && root.to > 0 ? (0 - root.from) / (root.to - root.from) * parent.width : 0
                property real curPos: slider.visualPosition * parent.width

                x: Math.min(zeroPos, curPos)
                width: Math.abs(curPos - zeroPos)
                height: parent.height
                color: root.accentColor
                radius: 2
            }
        }

        handle: Rectangle {
            x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
            y: slider.topPadding + slider.availableHeight / 2 - height / 2
            implicitWidth: 14
            implicitHeight: 14
            radius: 7
            color: slider.pressed ? root.accentColor : Theme.textMain
            border.color: Theme.bgDark
            border.width: 2

            Behavior on color { ColorAnimation { duration: 100 } }
        }

        onMoved: {
            root.value = slider.value
            root.sliderMoved(slider.value)
        }
    }
}
