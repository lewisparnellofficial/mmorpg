#include <QGuiApplication>
#include <QElapsedTimer>
#include <QFocusEvent>
#include <QMouseEvent>
#include <QPointingDevice>
#include <QProcess>
#include <QQuick3DGeometry>
#include <QQmlContext>
#include <QQuickView>
#include <QTabletEvent>
#include <QUrl>
#include <QVector3D>
#include <cstring>
#include <utility>

class HeightmapGeometry final : public QQuick3DGeometry {
    Q_OBJECT

public:
    explicit HeightmapGeometry(QQuick3DObject* parent = nullptr)
        : QQuick3DGeometry(parent)
    {
        setStride(static_cast<int>(sizeof(float) * 3));
        addAttribute(Attribute::PositionSemantic, 0, Attribute::F32Type);
        setPrimitiveType(PrimitiveType::Triangles);
    }

    void updateHeightmap(int width, int height, const QVector<float>& samples)
    {
        if (width < 2 || height < 2 || samples.size() != width * height) {
            return;
        }

        QByteArray vertices;
        vertices.resize(samples.size() * static_cast<int>(sizeof(float) * 3));
        auto* positions = reinterpret_cast<float*>(vertices.data());
        constexpr float horizontalExtent = 160.0f;
        constexpr float verticalExtent = 40.0f;
        for (int y = 0; y < height; ++y) {
            for (int x = 0; x < width; ++x) {
                const auto index = y * width + x;
                positions[index * 3] =
                    (static_cast<float>(x) / static_cast<float>(width - 1) - 0.5f)
                    * horizontalExtent;
                positions[index * 3 + 1] = samples[index] * verticalExtent;
                positions[index * 3 + 2] =
                    (static_cast<float>(y) / static_cast<float>(height - 1) - 0.5f)
                    * horizontalExtent;
            }
        }

        QByteArray indices;
        indices.resize((width - 1) * (height - 1) * 6 * static_cast<int>(sizeof(quint32)));
        auto* triangles = reinterpret_cast<quint32*>(indices.data());
        int cursor = 0;
        for (int y = 0; y < height - 1; ++y) {
            for (int x = 0; x < width - 1; ++x) {
                const auto topLeft = static_cast<quint32>(y * width + x);
                const auto topRight = topLeft + 1;
                const auto bottomLeft = static_cast<quint32>((y + 1) * width + x);
                const auto bottomRight = bottomLeft + 1;
                triangles[cursor++] = topLeft;
                triangles[cursor++] = bottomLeft;
                triangles[cursor++] = topRight;
                triangles[cursor++] = topRight;
                triangles[cursor++] = bottomLeft;
                triangles[cursor++] = bottomRight;
            }
        }

        setVertexData(vertices);
        setIndexData(indices);
        setBounds(QVector3D(-horizontalExtent / 2.0f, 0.0f, -horizontalExtent / 2.0f),
                  QVector3D(horizontalExtent / 2.0f, verticalExtent,
                             horizontalExtent / 2.0f));
    }
};

class TabletBridgeWindow final : public QQuickView {
    Q_OBJECT
    Q_PROPERTY(QString lastInput READ lastInput NOTIFY lastInputChanged)
    Q_PROPERTY(QString coreStatus READ coreStatus NOTIFY coreStatusChanged)

public:
    explicit TabletBridgeWindow(QWindow* parent = nullptr)
        : QQuickView(parent)
    {
        clock_.start();
        setResizeMode(QQuickView::SizeRootObjectToView);
        terrainGeometry_ = new HeightmapGeometry;
        rootContext()->setContextProperty(QStringLiteral("tabletBridge"), this);
        rootContext()->setContextProperty(QStringLiteral("terrainGeometry"), terrainGeometry_);
        startCoreBridge();
    }

    ~TabletBridgeWindow() override
    {
        if (core_.state() == QProcess::Running) {
            sendCommand(QStringLiteral("quit"));
            core_.waitForFinished(500);
        }
        delete terrainGeometry_;
    }

    QString lastInput() const { return lastInput_; }
    QString coreStatus() const { return coreStatus_; }

    Q_INVOKABLE void selectBrush(const QString& operation)
    {
        sendCommand(QStringLiteral("brush %1").arg(operation));
    }

    Q_INVOKABLE void setViewport(double x, double y, double width, double height)
    {
        viewportX_ = x;
        viewportY_ = y;
        viewportWidth_ = qMax(width, 1.0);
        viewportHeight_ = qMax(height, 1.0);
    }

    Q_INVOKABLE void saveTerrain()
    {
        sendCommand(QStringLiteral("save %1").arg(terrainPath()));
    }

    Q_INVOKABLE void reloadTerrain()
    {
        sendCommand(QStringLiteral("open %1").arg(terrainPath()));
    }

    Q_INVOKABLE void captureStroke()
    {
        sendCommand(QStringLiteral("capture %1").arg(capturePath()));
    }

    Q_INVOKABLE void undoTerrain() { sendCommand(QStringLiteral("undo")); }
    Q_INVOKABLE void redoTerrain() { sendCommand(QStringLiteral("redo")); }

signals:
    void lastInputChanged();
    void coreStatusChanged();
    void strokeStarted(double x, double y, double pressure, QString source);
    void strokePoint(double x, double y, double pressure, QString source);
    void strokeFinished(QString source);
    void strokeCancelled(QString reason);

protected:
    void tabletEvent(QTabletEvent* event) override
    {
        const auto phase = event->type();
        const auto point = event->position();
        const auto pressure = event->pressure();
        const auto source = event->pointerType() == QPointingDevice::PointerType::Eraser
            ? QStringLiteral("eraser")
            : QStringLiteral("pen");

        if (phase == QEvent::TabletEnterProximity) {
            setLastInput(QStringLiteral("%1 proximity-enter").arg(source));
            sendEvent(QStringLiteral("proximity-enter"), 0.0, 0.0, 0.0, source, 0.0, 0.0,
                      0.0);
        } else if (phase == QEvent::TabletLeaveProximity) {
            cancelStroke(QStringLiteral("proximity lost"));
            setLastInput(QStringLiteral("%1 proximity-leave").arg(source));
            sendEvent(QStringLiteral("proximity-leave"), 0.0, 0.0, 0.0, source, 0.0, 0.0,
                      0.0);
        } else if (phase == QEvent::TabletPress) {
            tabletStrokeActive_ = true;
            suppressMouseUntilRelease_ = true;
            emitSample(QStringLiteral("press"), point.x(), point.y(), pressure, source,
                       event->xTilt(), event->yTilt(), event->rotation());
        } else if (phase == QEvent::TabletMove && tabletStrokeActive_) {
            emitSample(QStringLiteral("move"), point.x(), point.y(), pressure, source,
                       event->xTilt(), event->yTilt(), event->rotation());
        } else if (phase == QEvent::TabletRelease && tabletStrokeActive_) {
            emitSample(QStringLiteral("release"), point.x(), point.y(), pressure, source,
                       event->xTilt(), event->yTilt(), event->rotation());
            tabletStrokeActive_ = false;
            // Qt may synthesize a compatibility mouse release after the
            // tablet release. Keep the suppression armed until that release
            // is consumed so one physical stroke cannot start a mouse stroke.
            suppressMouseUntilRelease_ = true;
            emit strokeFinished(source);
        }

        event->accept();
    }

    void focusOutEvent(QFocusEvent* event) override
    {
        cancelStroke(QStringLiteral("focus lost"));
        QQuickView::focusOutEvent(event);
    }

    void mousePressEvent(QMouseEvent* event) override
    {
        if (suppressMouseUntilRelease_) {
            event->accept();
            return;
        }
        mouseStrokeActive_ = true;
        emitSample(QStringLiteral("press"), event->position().x(), event->position().y(), 1.0,
                   QStringLiteral("mouse"), 0.0, 0.0, 0.0);
        event->accept();
    }

    void mouseMoveEvent(QMouseEvent* event) override
    {
        if (mouseStrokeActive_ && !suppressMouseUntilRelease_) {
            emitSample(QStringLiteral("move"), event->position().x(), event->position().y(), 1.0,
                       QStringLiteral("mouse"), 0.0, 0.0, 0.0);
        }
        event->accept();
    }

    void mouseReleaseEvent(QMouseEvent* event) override
    {
        if (suppressMouseUntilRelease_) {
            suppressMouseUntilRelease_ = false;
            event->accept();
            return;
        }
        if (mouseStrokeActive_) {
            emitSample(QStringLiteral("release"), event->position().x(), event->position().y(),
                       1.0, QStringLiteral("mouse"), 0.0, 0.0, 0.0);
            mouseStrokeActive_ = false;
            emit strokeFinished(QStringLiteral("mouse"));
        }
        event->accept();
    }

private:
    void setLastInput(const QString& value)
    {
        lastInput_ = value;
        emit lastInputChanged();
    }

    void cancelStroke(const QString& reason)
    {
        if (!tabletStrokeActive_ && !mouseStrokeActive_) {
            return;
        }
        tabletStrokeActive_ = false;
        mouseStrokeActive_ = false;
        suppressMouseUntilRelease_ = false;
        sendEvent(QStringLiteral("cancel"), 0.0, 0.0, 0.0, QStringLiteral("pen"), 0.0, 0.0,
                  0.0);
        emit strokeCancelled(reason);
    }

    void emitSample(const QString& phase, double x, double y, double pressure,
                    const QString& source, double tiltX, double tiltY, double rotation)
    {
        const auto timestamp = clock_.nsecsElapsed();
        setLastInput(QStringLiteral("%1 %2 x=%3 y=%4 pressure=%5 timestamp_ns=%6")
                         .arg(source, phase)
                         .arg(x, 0, 'f', 1)
                         .arg(y, 0, 'f', 1)
                         .arg(pressure, 0, 'f', 3)
                         .arg(timestamp));
        const auto documentPoint = toDocumentPoint(x, y);
        sendEvent(phase, documentPoint.first, documentPoint.second, pressure, source, tiltX,
                  tiltY, rotation, timestamp);
        if (phase == QStringLiteral("press")) {
            emit strokeStarted(x, y, pressure, source);
        } else if (phase == QStringLiteral("move")) {
            emit strokePoint(x, y, pressure, source);
        }
    }

    std::pair<double, double> toDocumentPoint(double x, double y) const
    {
        const auto normalizedX = qBound(0.0, (x - viewportX_) / viewportWidth_, 1.0);
        const auto normalizedY = qBound(0.0, (y - viewportY_) / viewportHeight_, 1.0);
        return {normalizedX * 31.0, normalizedY * 31.0};
    }

    void startCoreBridge()
    {
        const auto program = qEnvironmentVariable("MMORPG_EDITOR_CORE_BRIDGE",
                                                   QStringLiteral("target/debug/mmorpg-editor-core"));
        connect(&core_, &QProcess::readyReadStandardOutput, this, [this]() {
            coreOutputBuffer_.append(core_.readAllStandardOutput());
            while (true) {
                const auto newline = coreOutputBuffer_.indexOf('\n');
                if (newline < 0) {
                    break;
                }
                const auto line = QString::fromUtf8(coreOutputBuffer_.left(newline)).trimmed();
                coreOutputBuffer_.remove(0, newline + 1);
                handleCoreLine(line);
            }
        });
        connect(&core_, &QProcess::errorOccurred, this, [this](QProcess::ProcessError) {
            coreStatus_ = core_.errorString();
            emit coreStatusChanged();
        });
        core_.start(program, {QStringLiteral("--bridge")});
    }

    void handleCoreLine(const QString& line)
    {
        if (line.startsWith(QStringLiteral("preview "))) {
            const auto fields = line.split(' ', Qt::SkipEmptyParts);
            if (fields.size() >= 3) {
                bool widthOk = false;
                bool heightOk = false;
                const auto width = fields[1].toInt(&widthOk);
                const auto height = fields[2].toInt(&heightOk);
                QVector<float> samples;
                if (widthOk && heightOk && width > 1 && height > 1) {
                    samples.reserve(width * height);
                    for (int index = 3; index < fields.size(); ++index) {
                        bool ok = false;
                        const auto value = fields[index].toFloat(&ok);
                        if (!ok) {
                            samples.clear();
                            break;
                        }
                        samples.push_back(value);
                    }
                    if (samples.size() == width * height) {
                        terrainGeometry_->updateHeightmap(width, height, samples);
                    }
                }
            }
        }
        if (!line.isEmpty()) {
            coreStatus_ = line;
            emit coreStatusChanged();
        }
    }

    void sendEvent(const QString& phase, double x, double y, double pressure,
                   const QString& source, double tiltX, double tiltY, double rotation,
                   qint64 timestamp = -1)
    {
        if (timestamp < 0) {
            timestamp = clock_.nsecsElapsed();
        }
        sendCommand(QStringLiteral("event %1 %2 %3 %4 %5 %6 %7 %8 %9")
                        .arg(phase)
                        .arg(x, 0, 'f', 4)
                        .arg(y, 0, 'f', 4)
                        .arg(pressure, 0, 'f', 4)
                        .arg(source)
                        .arg(tiltX, 0, 'f', 4)
                        .arg(tiltY, 0, 'f', 4)
                        .arg(rotation, 0, 'f', 4)
                        .arg(timestamp));
    }

    void sendCommand(const QString& command)
    {
        if (core_.state() != QProcess::Running) {
            return;
        }
        core_.write(command.toUtf8());
        core_.write("\n");
        core_.waitForBytesWritten(100);
    }

    QString terrainPath() const
    {
        return qEnvironmentVariable("MMORPG_EDITOR_TERRAIN_OUTPUT",
                                    QStringLiteral("/tmp/mmorpg-editor-qt-terrain.mmterrain"));
    }

    QString capturePath() const
    {
        return qEnvironmentVariable("MMORPG_EDITOR_CAPTURE_OUTPUT",
                                    QStringLiteral("/tmp/mmorpg-editor-qt-stroke.mmstroke"));
    }

    QString lastInput_ = QStringLiteral("waiting for tablet or mouse input");
    bool tabletStrokeActive_ = false;
    bool mouseStrokeActive_ = false;
    bool suppressMouseUntilRelease_ = false;
    double viewportX_ = 0.0;
    double viewportY_ = 0.0;
    double viewportWidth_ = 1280.0;
    double viewportHeight_ = 800.0;
    QElapsedTimer clock_;
    QProcess core_;
    HeightmapGeometry* terrainGeometry_ = nullptr;
    QByteArray coreOutputBuffer_;
    QString coreStatus_ = QStringLiteral("starting Rust editor core bridge");
};

int main(int argc, char** argv)
{
    QGuiApplication application(argc, argv);
    TabletBridgeWindow window;
    window.setTitle(QStringLiteral("MMORPG Terrain Editor — Qt tablet proof"));
    window.setSource(QUrl(QStringLiteral("qrc:/qt/qml/Mmorpg/Editor/Main.qml")));
    window.resize(1280, 800);
    window.show();
    return application.exec();
}

#include "main.moc"
