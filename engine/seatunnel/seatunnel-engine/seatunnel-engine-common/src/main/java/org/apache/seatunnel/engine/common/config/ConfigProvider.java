/*
 * Copyright (c) 2008-2021, Hazelcast, Inc. All Rights Reserved.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

package org.apache.seatunnel.engine.common.config;

import com.hazelcast.client.config.ClientConfig;
import com.hazelcast.client.config.YamlClientConfigBuilder;
import com.hazelcast.client.config.impl.YamlClientConfigLocator;
import com.hazelcast.config.Config;
import com.hazelcast.config.YamlConfigBuilder;
import com.hazelcast.internal.config.YamlConfigLocator;
import lombok.NonNull;

import java.io.ByteArrayInputStream;
import java.net.InetAddress;
import java.net.NetworkInterface;
import java.net.SocketException;
import java.util.ArrayList;
import java.util.Enumeration;
import java.util.HashSet;
import java.util.List;
import java.util.Properties;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

import static com.hazelcast.internal.config.DeclarativeConfigUtil.SYSPROP_CLIENT_CONFIG;
import static com.hazelcast.internal.config.DeclarativeConfigUtil.SYSPROP_MEMBER_CONFIG;
import static com.hazelcast.internal.config.DeclarativeConfigUtil.validateSuffixInSystemProperty;
import static com.hazelcast.internal.util.StringUtil.isNullOrEmptyAfterTrim;

/**
 * Locates and loads SeaTunnel or SeaTunnel Client configurations from various locations.
 *
 * @see YamlSeaTunnelConfigLocator
 */
public final class ConfigProvider {

    /** Environment variable of the hazelcast member list, each item can be host or host:port. */
    private static final String ST_DOCKER_MEMBER_LIST = "ST_DOCKER_MEMBER_LIST";

    /**
     * Optional environment variable to explicitly set the public address (host:port) of the current
     * node, it has higher priority than the auto detection from {@link #ST_DOCKER_MEMBER_LIST}.
     */
    private static final String SEATUNNEL_SELF_ADDRESS = "SEATUNNEL_SELF_ADDRESS";

    /**
     * Optional environment variable to set the external address (host:port) of the current node. It
     * is reported to the health metrics for externally reachable urls (e.g. the log link), and does
     * not affect the cluster communication which keeps using the member list.
     */
    private static final String SEATUNNEL_EXTERNAL_ADDRESS = "SEATUNNEL_EXTERNAL_ADDRESS";

    /** Hazelcast config property key of the external address. */
    public static final String EXTERNAL_ADDRESS_PROPERTY = "seatunnel.external.address";

    /** Match the statefulset pod hostname like seatunnel-0, seatunnel-1. */
    private static final Pattern STATEFULSET_HOSTNAME_PATTERN = Pattern.compile(".*-(\\d+)$");

    private ConfigProvider() {}

    public static SeaTunnelConfig locateAndGetSeaTunnelConfig() {
        return locateAndGetSeaTunnelConfig(null);
    }

    @NonNull public static SeaTunnelConfig locateAndGetSeaTunnelConfig(Properties properties) {

        YamlSeaTunnelConfigLocator yamlConfigLocator = new YamlSeaTunnelConfigLocator();
        SeaTunnelConfig config;

        if (yamlConfigLocator.locateFromSystemProperty()) {
            // 1. Try loading YAML config if provided in system property
            config =
                    new YamlSeaTunnelConfigBuilder(yamlConfigLocator)
                            .setProperties(properties)
                            .build();

        } else if (yamlConfigLocator.locateInWorkDirOrOnClasspath()) {
            // 2. Try loading YAML config from the working directory or from the classpath
            config =
                    new YamlSeaTunnelConfigBuilder(yamlConfigLocator)
                            .setProperties(properties)
                            .build();
        } else {
            // 3. Loading the default YAML configuration file
            yamlConfigLocator.locateDefault();
            config =
                    new YamlSeaTunnelConfigBuilder(yamlConfigLocator)
                            .setProperties(properties)
                            .build();
        }
        return config;
    }

    public static SeaTunnelConfig locateAndGetSeaTunnelConfigFromString(String source) {
        return locateAndGetSeaTunnelConfigFromString(source, null);
    }

    @NonNull public static SeaTunnelConfig locateAndGetSeaTunnelConfigFromString(
            String source, Properties properties) {
        SeaTunnelConfig config;
        if (isNullOrEmptyAfterTrim(source)) {
            throw new IllegalArgumentException(
                    "provided string configuration is null or empty! "
                            + "Please use a well-structured content.");
        }
        byte[] bytes = source.getBytes();
        // Try loading YAML config from the source Text String
        config =
                new YamlSeaTunnelConfigBuilder(new ByteArrayInputStream(bytes))
                        .setProperties(properties)
                        .build();
        return config;
    }

    @NonNull public static ClientConfig locateAndGetClientConfig() {
        validateSuffixInSystemProperty(SYSPROP_CLIENT_CONFIG);

        ClientConfig config;
        YamlClientConfigLocator yamlConfigLocator = new YamlClientConfigLocator();

        if (yamlConfigLocator.locateFromSystemProperty()) {
            // 1. Try loading config if provided in system property, and it is an YAML file
            config = new YamlClientConfigBuilder(yamlConfigLocator.getIn()).build();
        } else if (yamlConfigLocator.locateInWorkDirOrOnClasspath()) {
            // 2. Try loading YAML config from the working directory or from the classpath
            config = new YamlClientConfigBuilder(yamlConfigLocator.getIn()).build();
        } else {
            // 3. Loading the default YAML configuration file
            yamlConfigLocator.locateDefault();
            config = new YamlClientConfigBuilder(yamlConfigLocator.getIn()).build();
        }
        // The client also reads the member list from the environment variable, so that the client
        // can connect to the cluster when the member addresses are not in the local config (e.g.
        // docker compose with host ip and port mapping). Each item can be host or host:port.
        String stDockerMemberList = System.getenv(ST_DOCKER_MEMBER_LIST);
        if (stDockerMemberList != null) {
            config.getNetworkConfig()
                    .setAddresses(
                            parseMemberList(stDockerMemberList, getClientDefaultPort(config)));
        }
        return config;
    }

    /**
     * Get the default hazelcast client port. It is extracted from the configured cluster members
     * (e.g. {@code localhost:5801} in hazelcast-client.yaml), falling back to the hazelcast default
     * port {@code 5701}.
     */
    private static int getClientDefaultPort(ClientConfig config) {
        for (String address : config.getNetworkConfig().getAddresses()) {
            if (hasExplicitPort(address)) {
                return Integer.parseInt(address.substring(address.lastIndexOf(':') + 1));
            }
        }
        return 5701;
    }

    @NonNull public static Config locateAndGetMemberConfig(Properties properties) {
        validateSuffixInSystemProperty(SYSPROP_MEMBER_CONFIG);

        Config config;
        YamlConfigLocator yamlConfigLocator = new YamlConfigLocator();

        if (yamlConfigLocator.locateFromSystemProperty()) {
            // 1. Try loading config if provided in system property, and it is an YAML file
            config =
                    new YamlConfigBuilder(yamlConfigLocator.getIn())
                            .setProperties(properties)
                            .build();
        } else if (yamlConfigLocator.locateInWorkDirOrOnClasspath()) {
            // 2. Try loading YAML config from the working directory or from the classpath
            config =
                    new YamlConfigBuilder(yamlConfigLocator.getIn())
                            .setProperties(properties)
                            .build();
        } else {
            // 3. Loading the default YAML configuration file
            yamlConfigLocator.locateDefault();
            config =
                    new YamlConfigBuilder(yamlConfigLocator.getIn())
                            .setProperties(properties)
                            .build();
        }
        applyDockerMemberList(config);
        applyExternalAddress(config);
        return config;
    }

    /**
     * Apply the member list from the environment variable {@code ST_DOCKER_MEMBER_LIST} to the
     * hazelcast config. Each item can be {@code host} or {@code host:port}; items without an
     * explicit port use the hazelcast port of this config. When the item of the current node is
     * found (by hostname, local ip or statefulset ordinal), its address is set as the public
     * address of this node, so that members can reach each other through the configured ip:port
     * (e.g. host ip with port mapping in docker compose).
     */
    private static void applyDockerMemberList(Config config) {
        String rawMemberList = System.getenv(ST_DOCKER_MEMBER_LIST);
        if (rawMemberList == null || rawMemberList.trim().isEmpty()) {
            return;
        }
        int defaultPort = config.getNetworkConfig().getPort();
        if (config.getNetworkConfig().getJoin().getTcpIpConfig().isEnabled()) {
            config.getNetworkConfig()
                    .getJoin()
                    .getTcpIpConfig()
                    .setMembers(parseMemberList(rawMemberList, defaultPort));
        }

        // The explicitly configured self address has the highest priority
        String selfAddress = System.getenv(SEATUNNEL_SELF_ADDRESS);
        if (selfAddress == null || selfAddress.trim().isEmpty()) {
            selfAddress = findSelfMemberAddress(rawMemberList, defaultPort);
        }
        if (selfAddress != null && !selfAddress.trim().isEmpty()) {
            config.getNetworkConfig().setPublicAddress(selfAddress.trim());
        }
    }

    /**
     * Set the external address (host:port) from the environment variable {@code
     * SEATUNNEL_EXTERNAL_ADDRESS} as a hazelcast config property, so that each node can report it
     * in the health metrics and generate externally reachable urls (e.g. the log link).
     */
    private static void applyExternalAddress(Config config) {
        String externalAddress = System.getenv(SEATUNNEL_EXTERNAL_ADDRESS);
        if (externalAddress != null && !externalAddress.trim().isEmpty()) {
            config.setProperty(EXTERNAL_ADDRESS_PROPERTY, externalAddress.trim());
        }
    }

    private static List<String> parseMemberList(String rawMemberList, int defaultPort) {
        List<String> members = new ArrayList<>();
        for (String item : rawMemberList.split(",")) {
            item = item.trim();
            if (item.isEmpty()) {
                continue;
            }
            members.add(hasExplicitPort(item) ? item : item + ":" + defaultPort);
        }
        return members;
    }

    private static boolean hasExplicitPort(String hostAndPort) {
        int index = hostAndPort.lastIndexOf(':');
        return index > 0 && hostAndPort.substring(index + 1).matches("\\d+");
    }

    /**
     * Find the address of the current node in the raw member list. Matching rules: {@code HOSTNAME}
     * env (exact or fqdn prefix), any local non-loopback ip, or the ordinal of a statefulset
     * hostname like {@code seatunnel-0}.
     */
    private static String findSelfMemberAddress(String rawMemberList, int defaultPort) {
        String hostname = System.getenv("HOSTNAME");
        Set<String> localIps = getLocalIps();
        String[] items = rawMemberList.split(",");
        for (String rawItem : items) {
            String item = rawItem.trim();
            if (item.isEmpty()) {
                continue;
            }
            String host = hasExplicitPort(item) ? item.substring(0, item.lastIndexOf(':')) : item;
            if (isSelfHost(host, hostname) || localIps.contains(host)) {
                return hasExplicitPort(item) ? item : item + ":" + defaultPort;
            }
        }
        if (hostname != null) {
            Matcher matcher = STATEFULSET_HOSTNAME_PATTERN.matcher(hostname);
            if (matcher.matches()) {
                int ordinal = Integer.parseInt(matcher.group(1));
                if (ordinal < items.length) {
                    String item = items[ordinal].trim();
                    if (!item.isEmpty()) {
                        return hasExplicitPort(item) ? item : item + ":" + defaultPort;
                    }
                }
            }
        }
        return null;
    }

    private static boolean isSelfHost(String host, String hostname) {
        if (hostname == null) {
            return false;
        }
        return host.equals(hostname) || host.startsWith(hostname + ".");
    }

    private static Set<String> getLocalIps() {
        Set<String> localIps = new HashSet<>();
        try {
            Enumeration<NetworkInterface> networkInterfaces =
                    NetworkInterface.getNetworkInterfaces();
            while (networkInterfaces.hasMoreElements()) {
                NetworkInterface networkInterface = networkInterfaces.nextElement();
                Enumeration<InetAddress> inetAddresses = networkInterface.getInetAddresses();
                while (inetAddresses.hasMoreElements()) {
                    InetAddress inetAddress = inetAddresses.nextElement();
                    if (!inetAddress.isLoopbackAddress() && !inetAddress.isLinkLocalAddress()) {
                        localIps.add(inetAddress.getHostAddress());
                    }
                }
            }
        } catch (SocketException e) {
            // ignore, fall back to hostname matching only
        }
        return localIps;
    }
}
